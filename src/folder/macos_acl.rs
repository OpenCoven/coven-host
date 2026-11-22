//! Validate a token file's extended ACL through its open descriptor.
use anyhow::{Context, Result, bail, ensure};
use libc::{c_int, c_void, uid_t};
use std::{fs::File, io, mem::MaybeUninit, os::fd::AsRawFd, ptr::NonNull};

// Darwin's <sys/acl.h>, <sys/fcntl.h> and <membership.h>.
const FILESEC_ACL: c_int = 5;
const ACL_FIRST_ENTRY: c_int = 0;
const ACL_NEXT_ENTRY: c_int = -1;
const ACL_EXTENDED_ALLOW: c_int = 1;
const ACL_EXTENDED_DENY: c_int = 2;

// XNU's well-known file-owner principal, distinct from the account UUID.
const FILE_OWNER: [u8; 16] = *uuid::uuid!("abcdefab-cdef-abcd-efab-cdef0000000a").as_bytes();
// membership.h documents the synthesized user UUID as this prefix plus a big-endian UID.
const SYNTHETIC_USER_PREFIX: [u8; 12] = [
    0xff, 0xff, 0xee, 0xee, 0xdd, 0xdd, 0xcc, 0xcc, 0xbb, 0xbb, 0xaa, 0xaa,
];

unsafe extern "C" {
    fn filesec_init() -> *mut c_void;
    fn filesec_free(security: *mut c_void);
    #[cfg_attr(target_arch = "x86_64", link_name = "fstatx_np$INODE64")]
    fn fstatx_np(fd: c_int, stat: *mut libc::stat, security: *mut c_void) -> c_int;
    fn filesec_get_property(security: *mut c_void, property: c_int, out: *mut c_void) -> c_int;
    fn acl_get_entry(acl: *mut c_void, index: c_int, entry: *mut *mut c_void) -> c_int;
    fn acl_get_tag_type(entry: *mut c_void, tag: *mut c_int) -> c_int;
    fn acl_get_qualifier(entry: *mut c_void) -> *mut c_void;
    fn acl_free(object: *mut c_void) -> c_int;
    fn mbr_uid_to_uuid(uid: uid_t, uuid: *mut u8) -> c_int;
}

struct FileSecurity(NonNull<c_void>);

impl Drop for FileSecurity {
    fn drop(&mut self) {
        // SAFETY: the pointer comes from filesec_init.
        unsafe { filesec_free(self.0.as_ptr()) };
    }
}

// Both the ACL and the individual qualifier returned by Darwin are owned copies.
struct AclAllocation(NonNull<c_void>);

impl Drop for AclAllocation {
    fn drop(&mut self) {
        // SAFETY: these pointers came from ACL functions that require acl_free.
        unsafe { acl_free(self.0.as_ptr()) };
    }
}

pub(super) fn check_token_acl(file: &File) -> Result<()> {
    // SAFETY: filesec_init takes no arguments and returns an allocated native object.
    let security = NonNull::new(unsafe { filesec_init() })
        .map(FileSecurity)
        .context("could not allocate macOS file security data")?;
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    // SAFETY: the borrowed descriptor, writable stat buffer, and file security object are live.
    if unsafe { fstatx_np(file.as_raw_fd(), stat.as_mut_ptr(), security.0.as_ptr()) } != 0 {
        return Err(io::Error::last_os_error())
            .context("could not inspect the token file's macOS ACL");
    }
    let mut acl = std::ptr::null_mut::<c_void>();
    // SAFETY: FILESEC_ACL writes an owned acl_t into the supplied pointer-sized slot.
    if unsafe { filesec_get_property(security.0.as_ptr(), FILESEC_ACL, (&raw mut acl).cast()) } != 0
    {
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ENOENT) {
            // Descriptor inspection succeeded; only the optional ACL property is absent.
            return Ok(());
        }
        return Err(error).context("could not verify the token file's macOS ACL");
    }
    let acl = NonNull::new(acl)
        .map(AclAllocation)
        .context("macOS returned an empty ACL handle")?;

    // The preceding native owner check established that this UID owns the file.
    // SAFETY: geteuid has no arguments or memory preconditions.
    let uid = unsafe { libc::geteuid() };
    let mut index = ACL_FIRST_ENTRY;
    loop {
        let mut entry = std::ptr::null_mut();
        // SAFETY: acl owns a live native ACL. Darwin fills entry with a borrowed pointer.
        if unsafe { acl_get_entry(acl.0.as_ptr(), index, &mut entry) } != 0 {
            let error = io::Error::last_os_error();
            // Darwin constructs a valid ACL for FILESEC_ACL, and FIRST/NEXT returns EINVAL
            // when that iterator is exhausted.
            if error.raw_os_error() == Some(libc::EINVAL) {
                return Ok(());
            }
            return Err(error).context("could not enumerate the token file's macOS ACL");
        }
        index = ACL_NEXT_ENTRY;
        let mut tag = 0;
        // SAFETY: entry is borrowed from the live ACL and tag is a writable C int.
        if unsafe { acl_get_tag_type(entry, &mut tag) } != 0 {
            return Err(io::Error::last_os_error()).context("could not inspect a token ACL entry");
        }
        match tag {
            ACL_EXTENDED_DENY => continue,
            ACL_EXTENDED_ALLOW => {}
            _ => bail!("unsupported entry in the token file's macOS ACL"),
        }

        // SAFETY: for an allow entry, Darwin returns a separately allocated 16-byte UUID.
        let qualifier = NonNull::new(unsafe { acl_get_qualifier(entry) })
            .map(AclAllocation)
            .context("could not identify a token ACL principal")?;
        // SAFETY: qualifier points to a live 16-byte UUID and this array has byte alignment.
        let principal = unsafe { qualifier.0.cast::<[u8; 16]>().as_ref() };
        if *principal == FILE_OWNER
            || (principal.starts_with(&SYNTHETIC_USER_PREFIX)
                && principal[12..] == uid.to_be_bytes())
        {
            continue;
        }
        let mut owner = [0u8; 16];
        // SAFETY: owner provides writable storage for the UUID.
        let status = unsafe { mbr_uid_to_uuid(uid, owner.as_mut_ptr()) };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status))
                .context("could not identify the token file owner for its macOS ACL");
        }
        ensure!(
            *principal == owner,
            "expected an owner-only token file; its macOS ACL grants access to another principal"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{ffi::CString, io::Write};

    const ACL_TYPE_EXTENDED: c_int = 0x100;

    unsafe extern "C" {
        fn acl_from_text(text: *const libc::c_char) -> *mut c_void;
        fn acl_set_fd_np(fd: c_int, acl: *mut c_void, kind: c_int) -> c_int;
    }

    #[test]
    fn accepts_darwin_owner_principals() -> Result<()> {
        let base = tempfile::tempdir()?;
        let mut file = tempfile::NamedTempFile::new_in(base.path().canonicalize()?)?;
        let contents = "x".repeat(43);
        file.write_all(contents.as_bytes())?;
        // SAFETY: geteuid has no arguments or memory preconditions.
        let uid = unsafe { libc::geteuid() };
        for principal in [
            "ABCDEFAB-CDEF-ABCD-EFAB-CDEF0000000A".to_owned(),
            format!("FFFFEEEE-DDDD-CCCC-BBBB-AAAA{uid:08X}"),
        ] {
            let text = CString::new(format!("!#acl 1\nuser:{principal}:::allow:read\n"))?;
            // SAFETY: text is a live NUL-terminated buffer, and Darwin allocates the ACL.
            let acl = NonNull::new(unsafe { acl_from_text(text.as_ptr()) })
                .map(AclAllocation)
                .context("could not construct owner ACL fixture")?;
            // SAFETY: the file and the test ACL remain live for the call.
            if unsafe {
                acl_set_fd_np(
                    file.as_file().as_raw_fd(),
                    acl.0.as_ptr(),
                    ACL_TYPE_EXTENDED,
                )
            } != 0
            {
                return Err(io::Error::last_os_error())
                    .context("could not apply owner ACL fixture");
            }
            assert_eq!(crate::folder::token_from(file.path())?, contents);
        }
        Ok(())
    }
}
