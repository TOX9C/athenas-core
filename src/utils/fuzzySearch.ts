export function fuzzySearch(query: string, items: string[]): string[] {
  if (!query) return items
  const lower = query.toLowerCase()
  return items
    .filter((item) => {
      const itemLower = item.toLowerCase()
      let qi = 0
      for (let i = 0; i < itemLower.length && qi < lower.length; i++) {
        if (itemLower[i] === lower[qi]) qi++
      }
      return qi === lower.length
    })
    .sort((a, b) => {
      const aLower = a.toLowerCase()
      const bLower = b.toLowerCase()
      const aStartsWith = aLower.startsWith(lower) ? 0 : 1
      const bStartsWith = bLower.startsWith(lower) ? 0 : 1
      if (aStartsWith !== bStartsWith) return aStartsWith - bStartsWith
      return a.length - b.length
    })
}
