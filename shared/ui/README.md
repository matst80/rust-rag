Shared UI components for rust-rag

This package contains lightweight, framework-agnostic React components that can be reused by both the Next.js `frontend` and the `chrome-extension`.

Current exports:
- `TabBar` - simple tab bar used in the extension UI
- `StatusBar` - footer status bar with online/offline and settings button
- `SettingsModal` - basic settings modal (no device auth included)
- `Spinner` - small spinner component

How to use (local development)

1. From each package (e.g. `frontend` or `chrome-extension`) add a file dependency:

```json
"dependencies": {
  "@rust-rag/ui": "file:../shared/ui"
}
```

2. Run your package manager (npm/pnpm/yarn) to install the local dependency. For example:

```sh
cd chrome-extension
npm install
```

3. Import components in your code:

```ts
import { TabBar, StatusBar } from '@rust-rag/ui';
```

Notes
- The package ships a prebuilt `dist` ESM build so bundlers should be able to consume it without requiring you to transpile the `src` sources.
- The components intentionally keep the same CSS class names used by the existing chrome-extension styles so migration can be gradual.
