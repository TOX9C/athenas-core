# File Explorer Design Specification

**Goal:** Implement a recursive file tree in the left sidebar that displays the exact directory structure of the user's active workspace, utilizing the existing `window.athena.fs.readTree` backend logic.

## Architecture & Data Flow

1. **Data Source:** 
   We will rely on the existing Electron `fs:readTree` IPC handler. The frontend accesses this via `window.athena.fs.readTree(dir: string)`, which returns an array of `FileNode` interfaces (depth up to 6, already ignoring node_modules, .git, etc.).

2. **State Management:**
   Since file data strictly depends on `activeSpace.dir`, the file tree data will be stored in component-level React state (e.g., `useState<FileNode[]>`) inside a new `FileTree` component, hydrated inside a `useEffect` whenever the active space directory changes.

3. **Components:**
   We will break this down into two main recursive components:
   - `FileTree`: The root component that fetches the data from Electron, manages loading states, and renders a list of `FileNodeItem`s.
   - `FileNodeItem`: A recursive component that renders a single file or a folder. If it is a folder, it manages its own local `isOpen` state and recursively maps its `children` property to more `FileNodeItem`s.

4. **Integration:**
   The `src/components/Sidebar/Sidebar.tsx` will have its "File explorer coming soon" placeholder removed and replaced with `<FileTree dir={activeSpace?.dir} />`.

## UI & Styling

- **Icons:** We will use the existing `lucide-react` icons.
  - Folders (Open): `ChevronDown` + `FolderOpen`
  - Folders (Closed): `ChevronRight` + `Folder`
  - Files: `File` code icon or a generic text file icon depending on extension logic if desired (fallback to `File`).
- **Indentation:** Using Tailwind CSS `pl-4` (padding left) recursively on children to create hierarchy.
- **Interactions:** Clicking a folder toggles its `isOpen` state. Click a file currently does nothing but visually highlighting (as editor functionality integration is not requested yet, but hover states will be distinct).

## Error Handling

- If `workspace.dir` is missing, the component returns `null` or a generic "No workspace active" message.
- If `window.athena.fs.readTree` throws an error or fails, an error state `div` will inform the user.
- A loading spinner will indicate the async fetch is occurring during switching spaces.
