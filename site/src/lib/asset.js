// 把 public/ 下的静态资源解析为带部署 base 前缀的 URL。
// 这样无论站点部署在根域名（base '/'）还是子路径（如 '/untype/'），
// 图片都能正确加载。import.meta.env.BASE_URL 始终以 '/' 结尾。
export const asset = (path) => import.meta.env.BASE_URL + String(path).replace(/^\/+/, '')
