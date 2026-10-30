// Capture the rendered result cards, preserving their computed appearance.
// Only the detached clone is expanded; the live page and its scroll stay intact.
export async function resultImage(source: HTMLElement, english: boolean): Promise<Blob> {
  const message = (zh: string, en: string) => english ? en : zh
  await document.fonts.ready
  const width = Math.ceil(source.getBoundingClientRect().width)
  const clone = source.cloneNode(true) as HTMLElement
  const nodes = [source, ...source.querySelectorAll<HTMLElement>('*')]
  const copies = [clone, ...clone.querySelectorAll<HTMLElement>('*')]
  nodes.forEach((node, i) => {
    const copy = copies[i]!
    const style = getComputedStyle(node)
    for (const property of style) copy.style.setProperty(property, style.getPropertyValue(property))
    copy.style.animation = 'none'
    copy.style.transition = 'none'
    // Sticky headers otherwise retain their offset inside the scroll container.
    if (style.position === 'sticky') copy.style.position = 'static'
    if (node instanceof HTMLInputElement) {
      const text = document.createElement('span')
      text.style.cssText = copy.style.cssText
      text.textContent = node.value || node.placeholder
      if (!node.value) text.style.color = getComputedStyle(node, '::placeholder').color
      copy.replaceWith(text)
    }
  })
  clone.querySelectorAll('.task-panel,.result-tools,.raw-result,[role="alert"]').forEach(node => node.remove())
  clone.querySelectorAll<HTMLElement>('.result-panel').forEach(node => {
    node.style.height = 'auto'; node.style.maxHeight = 'none'
  })
  clone.querySelectorAll<HTMLElement>('.panel-title h2,.result-meta code,.result-meta span').forEach(node => {
    node.style.width = 'auto'; node.style.height = 'auto'
  })
  clone.querySelectorAll<HTMLElement>('.table-scroll').forEach(node => {
    node.style.height = 'auto'; node.style.maxHeight = 'none'; node.style.overflow = 'visible'
  })
  clone.style.width = `${width}px`
  clone.style.minWidth = '0'
  clone.style.maxWidth = 'none'
  clone.style.height = 'auto'
  const wrapper = document.createElement('div')
  wrapper.style.cssText = `position:fixed;left:-100000px;top:0;width:${width + 32}px;padding:16px;box-sizing:border-box;background:${getComputedStyle(document.documentElement).backgroundColor};pointer-events:none;`
  wrapper.setAttribute('aria-hidden', 'true')
  const brand = document.createElement('div')
  brand.textContent = 'HopKit · Network Toolkit'
  brand.style.cssText = `font:600 16px sans-serif;color:${getComputedStyle(source).color};padding:0 0 12px;`
  wrapper.append(brand)
  wrapper.append(clone)
  document.body.append(wrapper)
  let height: number
  let markup: string
  try {
    height = Math.ceil(wrapper.getBoundingClientRect().height)
    if (height > 28000 || width < 1 || (width + 32) * height > 32_000_000) {
      throw new Error(message('结果过长，请筛选后截图或导出 JSON。', 'Results are too long. Filter the results or export JSON.'))
    }
    // Position belongs only to the measurement wrapper, not the image.
    wrapper.style.position = 'static'; wrapper.style.left = 'auto'; wrapper.style.top = 'auto'
    markup = new XMLSerializer().serializeToString(wrapper)
  } finally { wrapper.remove() }
  const w = width + 32
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${height}"><foreignObject width="100%" height="100%">${markup}</foreignObject></svg>`
  const image = new Image()
  await new Promise<void>((resolve, reject) => {
    image.onload = () => resolve()
    image.onerror = () => reject(new Error(message('截图生成失败。', 'Screenshot generation failed.')))
    image.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg)
  })
  const canvas = document.createElement('canvas')
  const scale = Math.min(2, Math.sqrt(32_000_000 / (w * height)), 28000 / height)
  canvas.width = Math.floor(w * scale); canvas.height = Math.floor(height * scale)
  const context = canvas.getContext('2d')
  if (!context) throw new Error(message('无法创建截图。', 'Cannot create screenshot.'))
  context.scale(scale, scale)
  context.drawImage(image, 0, 0)
  const url = canvas.toDataURL('image/png')
  const bytes = Uint8Array.from(atob(url.slice(url.indexOf(',') + 1)), c => c.charCodeAt(0))
  if (bytes.length > 16 * 1024 * 1024) throw new Error(message('图片超过 16 MiB，请筛选结果后重试。', 'Image exceeds 16 MiB. Filter results and try again.'))
  return new Blob([bytes], { type: 'image/png' })
}
