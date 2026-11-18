use super::wide;
use anyhow::{Result, ensure};
use std::ffi::{OsStr, c_void};
use std::fs::File;
use std::io;
use std::mem::size_of;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::Path;
use std::ptr::{addr_of, null_mut};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LocalFree};
use windows_sys::Win32::Security::Authorization::*;
use windows_sys::Win32::Security::*;
use windows_sys::Win32::Storage::FileSystem::CreateDirectoryW;
use windows_sys::Win32::System::SystemServices::{ACCESS_ALLOWED_ACE_TYPE, ACCESS_DENIED_ACE_TYPE};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}

// The SID returned in TOKEN_USER points into the same aligned allocation.
struct User(Vec<usize>);
impl User {
    fn current() -> Result<Self> {
        let mut token = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let token = unsafe { OwnedHandle::from_raw_handle(token) };
        let mut length = 0;
        unsafe {
            GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut length)
        };
        let err = io::Error::last_os_error();
        ensure!(
            err.raw_os_error() == Some(ERROR_INSUFFICIENT_BUFFER as i32)
                && length as usize >= size_of::<TOKEN_USER>(),
            "cannot size Windows user token: {err}"
        );
        let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } == 0
        {
            return Err(io::Error::last_os_error().into());
        }
        Ok(Self(buffer))
    }

    fn sid(&self) -> PSID {
        unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    fn sid_string(&self) -> Result<String> {
        let mut string = null_mut();
        if unsafe { ConvertSidToStringSidW(self.sid(), &mut string) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let _hold = Local(string.cast());
        let mut len = 0;
        while unsafe { *string.add(len) } != 0 {
            len += 1;
        }
        Ok(String::from_utf16(unsafe {
            std::slice::from_raw_parts(string, len)
        })?)
    }
}

pub(super) fn mkdir(path: &Path) -> Result<()> {
    let sid = User::current()?.sid_string()?;
    let sddl = format!("O:{sid}D:P(A;OICI;FA;;;{sid})(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
    let text = wide(OsStr::new(&sddl))?;
    let mut descriptor = null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error().into());
    }
    let _hold = Local(descriptor);
    let attrs = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let name = wide(path.as_os_str())?;
    if unsafe { CreateDirectoryW(name.as_ptr(), &attrs) } == 0 {
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::AlreadyExists {
            return Err(err.into());
        }
    }
    Ok(())
}

pub(in crate::folder) fn check_private(file: &File) -> Result<()> {
    let user = User::current()?;
    let (mut owner, mut acl, mut descriptor) = (null_mut(), null_mut(), null_mut());
    let result = unsafe {
        GetSecurityInfo(
            file.as_raw_handle(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut descriptor,
        )
    };
    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result as i32).into());
    }
    let _hold = Local(descriptor);
    ensure!(
        !owner.is_null() && unsafe { EqualSid(owner, user.sid()) } != 0,
        "private file or directory must be owned by the current user"
    );
    ensure!(
        !acl.is_null() && unsafe { IsValidAcl(acl) } != 0,
        "a private Windows DACL is required"
    );
    for i in 0..unsafe { (*acl).AceCount } {
        let mut ace = null_mut();
        if unsafe { GetAce(acl, u32::from(i), &mut ace) } == 0 {
            return Err(io::Error::last_os_error().into());
        }
        let header = unsafe { &*ace.cast::<ACE_HEADER>() };
        if u32::from(header.AceType) == ACCESS_DENIED_ACE_TYPE {
            continue;
        }
        ensure!(
            u32::from(header.AceType) == ACCESS_ALLOWED_ACE_TYPE,
            "unsupported ACE in private Windows DACL"
        );
        let entry = ace.cast::<ACCESS_ALLOWED_ACE>();
        let sid = unsafe { addr_of!((*entry).SidStart) }.cast_mut().cast();
        ensure!(
            unsafe {
                EqualSid(sid, user.sid()) != 0
                    || IsWellKnownSid(sid, WinLocalSystemSid) != 0
                    || IsWellKnownSid(sid, WinBuiltinAdministratorsSid) != 0
                    || IsWellKnownSid(sid, WinCreatorOwnerRightsSid) != 0
                    || (u32::from(header.AceFlags) & INHERIT_ONLY_ACE != 0
                        && IsWellKnownSid(sid, WinCreatorOwnerSid) != 0)
            },
            "private Windows DACL grants access to another principal"
        );
    }
    Ok(())
}
