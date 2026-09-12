## Parent

https://github.com/helloworldx55/cursor-api-proxy/issues/1

## What to build

首次为 Operator 生成 Bridge Token 并写入系统凭据库。Caller 不带合法 Bearer 会被拒绝。「复制 Caller 配置」给出带 Bound Port 的 Base URL 和 Token。可轮换 Token。日志与复制区以外不得长期明文展示 Token。

## Acceptance criteria

- [ ] 首次就绪时生成 Bridge Token 并存入操作系统凭据库
- [ ] 无 Bearer 或错误 Bearer 的 Caller 请求失败
- [ ] 复制出的配置含 `http://127.0.0.1:<Bound Port>/v1` 与当前 Token
- [ ] 轮换 Token 后旧 Token 失效
- [ ] 日志文件中不出现 Bridge Token

## Blocked by

- #2 启停本机 Bridge
