const fs = require('fs');
const path = './src/components/Workspace/NewSpaceModal.tsx';
let code = fs.readFileSync(path, 'utf8');

// Ensure themes is imported
if (!code.includes("import { themes }")) {
  code = code.replace("import { useSwarmStore } from '../../store/swarmStore'", "import { useSwarmStore } from '../../store/swarmStore'\nimport { themes } from '../../themes/themes'");
}

// Add theme to useUIStore extraction
code = code.replace("const { setActivePanel } = useUIStore()", "const { setActivePanel, theme } = useUIStore()");

// Update xterm initialization with correct theme colors
const xtermInitRegex = /term = new Terminal\(\{[\s\S]*?cursorBlink: true\n\s*\}\)/;
const newXtermInit = `const themeColors = themes[theme]?.colors;
          term = new Terminal({
            rows: 1,
            fontFamily: 'monospace',
            fontSize: 12,
            theme: themeColors ? {
              background: 'transparent',
              foreground: themeColors.text || themeColors.terminalFg,
              cursor: themeColors.accent || themeColors.terminalCursor,
              selectionBackground: themeColors.terminalSelection,
            } : { background: 'transparent' },
            cursorBlink: true
          })`;
code = code.replace(xtermInitRegex, newXtermInit);

// Add focus to the Xterm container
const terminalContainerRegex = /<div\s+ref=\{termRef\}\s+className="w-full px-2 py-1.5 rounded-lg text-xs"\s+style=\{\{ background: 'var\(--bgTertiary\)', border: '1px solid var\(--border\)', minHeight: '34px', overflow: 'hidden' \}\}\s+>/;
const newTerminalContainer = `<div
            ref={termRef}
            onClick={() => termInstance?.focus()}
            className="w-full px-2 py-1.5 rounded-lg text-xs cursor-text"
            style={{ background: 'var(--bgTertiary)', border: '1px solid var(--border)', minHeight: '34px', overflow: 'hidden' }}
          >`;
code = code.replace(terminalContainerRegex, newTerminalContainer);

// Auto-focus on ready
const spawnRegex = /termInstance.__cleanup = \(\) => \{\n\s*cleanupData\(\)\n\s*dataHandler\.dispose\(\)\n\s*\}/;
const newSpawn = `termInstance.__cleanup = () => {
          cleanupData()
          dataHandler.dispose()
        }
        
        // Auto-focus terminal to allow immediate typing
        setTimeout(() => {
          if (termInstance) {
             termInstance.focus()
          }
        }, 100)`;
code = code.replace(spawnRegex, newSpawn);

fs.writeFileSync(path, code);
