let storeInstance: any = null
export async function getStore() {
  if (!storeInstance) {
    const { default: Store } = await import('electron-store')
    storeInstance = new Store()
  }
  return storeInstance
}
