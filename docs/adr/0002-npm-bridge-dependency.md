# 用 npm 依赖 cursor-api-proxy，不分叉 Bridge

cursor2api 是独立产品（品牌、Release、Console 体验、更安全的默认值），但 HTTP 翻译层不是我们的核心。我们把 `cursor-api-proxy` 当作 npm 依赖打进安装包，而不是在本仓库 vendor 或分叉上游。协议漂移仍由上游承担，除非以后我们需要而他们不接受的 Bridge 行为。
