# Error Handling

当认证失败时，服务应该返回明确的错误码，并记录用户标识和请求上下文。

如果 token 缺失，则返回 401 Unauthorized。
