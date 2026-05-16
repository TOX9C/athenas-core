// ---------------------------------------------------------------------------
// Layout state has been consolidated into UIState (stores::ui) and
// PanelManagerState (stores::panel_manager).
//
// SidebarSection is now defined in stores::ui.
// ExclusivePanel is now defined in stores::panel_manager.
//
// This module is kept as a thin compatibility shim so that existing
// `provide_layout_store()` calls do not break during migration.
// It simply ensures the UI store and panel manager store are initialized.
// ---------------------------------------------------------------------------

/// Legacy layout store initializer — now a no-op wrapper that delegates
/// to the canonical store providers.
pub fn provide_layout_store() {
    // The actual stores are already provided by App before this is called.
    // This function is kept only for backward compatibility during migration.
}
