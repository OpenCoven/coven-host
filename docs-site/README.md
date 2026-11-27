# Publish Coven Host Docs

From this repository, build the public guides and shared brand assets into a fresh static directory with Node.js 22 or later. Use the existing Vercel project under the 0xbuns team.

```sh
cd docs-site
npm ci
npm run build
vercel deploy dist --scope 0xbuns --project coven-host-docs --prod --yes
```

Open [the production site](https://coven-host-docs.vercel.app) after deployment and check the install guide, SSH section and conflict recovery link. The custom domain is [docs.coven.host](https://docs.coven.host). Vercel manages its certificate once the external DNS record points to the project.
