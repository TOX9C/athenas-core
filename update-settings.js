const fs = require('fs');
const path = './src/components/Settings/SettingsModal.tsx';
let code = fs.readFileSync(path, 'utf8');

// Ensure FolderOpen is imported
if (!code.includes('FolderOpen')) {
  code = code.replace(/import \{ X, /, 'import { X, FolderOpen, ');
}

// Add state
const addStateStr = `  const [apiKey, setApiKey] = useState('')
  const [defaultWorkspaceDir, setDefaultWorkspaceDir] = useState('')`;
code = code.replace(/  const \[apiKey, setApiKey\] = useState\(''\)/, addStateStr);

// Add useEffect logic
const useEffectReplacement = `    window.athena.store.get('athena.apiKey').then((val: any) => {
      setApiKey(val || '')
    })
    window.athena.store.get('athena-defaultWorkspaceDir').then((val: any) => {
      if (val) setDefaultWorkspaceDir(val)
    })`;
code = code.replace(/    window\.athena\.store\.get\('athena\.apiKey'\)\.then\(\(val: any\) => \{\n      setApiKey\(val \|\| ''\)\n    \}\)/, useEffectReplacement);

// Add component row
const rowStr = `                <SettingRow label="Default Workspace">
                  <div className="flex items-center gap-2">
                    <input
                      value={defaultWorkspaceDir}
                      onChange={(e) => {
                        setDefaultWorkspaceDir(e.target.value)
                        window.athena.store.set('athena-defaultWorkspaceDir', e.target.value)
                      }}
                      placeholder="/Users/my/projects"
                      className="px-2 py-1 rounded w-48 text-xs outline-none bg-transparent"
                      style={{ border: '1px solid var(--border)', color: 'var(--text)' }}
                    />
                    <button
                      onClick={async () => {
                        const selected = await window.athena.fs.showOpenDialog()
                        if (selected) {
                          setDefaultWorkspaceDir(selected)
                          window.athena.store.set('athena-defaultWorkspaceDir', selected)
                        }
                      }}
                      className="px-2 py-1 rounded text-[11px] transition-colors flex items-center justify-center"
                      style={{ background: 'var(--bgTertiary)', border: '1px solid var(--border)', color: 'var(--text)' }}
                    >
                      <FolderOpen size={12} className="mr-1" /> Browse
                    </button>
                  </div>
                </SettingRow>
                <SettingRow label="Font family">`;

code = code.replace(/                <SettingRow label="Font family">/, rowStr);

fs.writeFileSync(path, code);
