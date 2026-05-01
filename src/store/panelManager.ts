export type ExclusivePanel = 'athena' | 'browser' | 'editor' | null

type PanelStateSource = {
  getState: () => { browserOpen: boolean; editorOpen: boolean }
}

type UIStateSource = PanelStateSource & {
  setState: (partial: any) => void
}

type AthenaStateSource = {
  getState: () => { isOpen: boolean }
  setState: (partial: { isOpen: boolean }) => void
}

let _uiStore: UIStateSource | null = null
let _athenaStore: AthenaStateSource | null = null

export function registerUIStore(ui: UIStateSource): void {
  _uiStore = ui
}

export function registerAthenaStore(athena: AthenaStateSource): void {
  _athenaStore = athena
}

function applyActivation(panel: ExclusivePanel): void {
  if (!_uiStore || !_athenaStore) return

  const browserOpen = panel === 'browser'
  const editorOpen = panel === 'editor'
  const athenaOpen = panel === 'athena'

  _uiStore.setState({
    browserOpen,
    editorOpen: editorOpen && !browserOpen,
  })
  _athenaStore.setState({ isOpen: athenaOpen })
}

export function activatePanel(panel: ExclusivePanel): void {
  applyActivation(panel)
}

export function togglePanel(panel: ExclusivePanel): void {
  if (!_uiStore || !_athenaStore) return

  const uiState = _uiStore.getState()
  const athenaState = _athenaStore.getState()

  const isCurrentlyOpen =
    panel === 'browser'
      ? uiState.browserOpen
      : panel === 'editor'
        ? uiState.editorOpen
        : panel === 'athena'
          ? athenaState.isOpen
          : false

  activatePanel(isCurrentlyOpen ? null : panel)
}
