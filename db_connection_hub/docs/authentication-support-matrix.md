# 认证方式支持矩阵（主流数据库）

说明：
- `MVP`：当前阶段必须支持
- `Phase-2`：第二阶段支持
- `N/A`：该数据库不适用

## 1. 统一认证模式定义

- `password`：账号 + 密码
- `tls_client_cert`：客户端证书（mTLS）
- `token`：短期 token（例如云厂商 IAM token）
- `integrated`：系统集成认证（Kerberos/SSPI/AD）
- `file_key`：文件或 wallet/key 方式
- `no_auth`：无需认证（典型 SQLite）

## 2. 数据库认证支持表

| DB Type | password | tls_client_cert | token | integrated | file_key | no_auth |
|---|---|---|---|---|---|---|
| PostgreSQL | MVP | MVP | Phase-2（RDS IAM） | Phase-2（GSSAPI/Kerberos） | N/A | N/A |
| MySQL/MariaDB | MVP | MVP | Phase-2（RDS IAM） | Phase-2（企业 SSO） | N/A | N/A |
| SQL Server | MVP（SQL Login） | MVP（TLS） | Phase-2（Azure AD Token） | Phase-2（AD/Kerberos） | N/A | N/A |
| Oracle | MVP | MVP（TCPS/Wallet） | Phase-2 | Phase-2（Kerberos） | MVP（Wallet） | N/A |
| MongoDB | MVP（SCRAM） | MVP（X.509） | Phase-2（OIDC/Atlas） | Phase-2 | N/A | N/A |
| SQLite | N/A | N/A | N/A | N/A | Phase-2（SQLCipher Key） | MVP |

## 3. 网络与传输安全组合

认证方式与网络方式可组合，前端向导按选择动态展示：

1. 网络模式
- `direct`：直接连接
- `ssh_tunnel`：通过 SSH 隧道连接
- `proxy`：通过数据库代理（例如 PgBouncer）

2. 传输加密
- `disabled`
- `preferred`
- `required`
- `verify_ca`
- `verify_full`

3. 证书材料
- CA 证书
- Client Cert
- Client Key
- Key Passphrase（可选）

## 4. 连接向导字段设计（按步骤）

1. Step 1: DB 类型与名称
- db_type
- datasource_name
- tags

2. Step 2: 网络参数
- host / port / service_name / sid / file_path
- network_mode（direct/ssh_tunnel/proxy）
- ssh 配置（host, port, user, auth）

3. Step 3: 认证方式
- auth_mode（根据 db_type 动态可选）
- username/password 或 token 或 cert/wallet

4. Step 4: TLS
- ssl_mode
- ca_cert/client_cert/client_key

5. Step 5: 高级参数
- connect_timeout_ms
- pool_min / pool_max
- statement_timeout_ms

6. Step 6: 测试与保存
- 网络联通检查
- 认证检查
- 权限检查（是否有 metadata 读取权限）

## 5. 错误分类（连接创建/测试）

统一错误码建议：
- `CONN_NETWORK_UNREACHABLE`
- `CONN_TLS_HANDSHAKE_FAILED`
- `CONN_AUTH_FAILED`
- `CONN_DB_NOT_FOUND`
- `CONN_PERMISSION_DENIED`
- `CONN_TIMEOUT`
- `CONN_DRIVER_NOT_SUPPORTED`

前端展示建议：
- 错误信息 + 修复建议 + 重试按钮
- 展示“失败阶段”（网络/认证/权限）

## 6. 凭据存储与轮换

1. 明文不落库：password/token/key/wallet_path 对应密文或安全引用
2. 密钥管理：本地 AES-GCM（MVP）+ 外部 KMS（Phase-2）
3. 凭据轮换：支持“更新凭据不改连接 ID”
4. 审计：记录凭据变更事件，不记录敏感内容

## 7. 各认证模式必填字段（后端校验规则）

### 7.1 password

必填：
- `username`
- `password`

适用：PostgreSQL / MySQL / SQL Server / Oracle / MongoDB（SCRAM）

### 7.2 tls_client_cert

必填：
- `client_cert`
- `client_key`
- `tls.ca_cert`（建议必填）

可选：
- `username`（某些数据库仍需）

适用：PostgreSQL / MySQL / Oracle / MongoDB / SQL Server（按驱动能力）

### 7.3 token

必填：
- `access_token`

可选：
- `token_expire_at`
- `token_provider`（aws_iam / azure_ad / custom）

适用：云托管数据库（Phase-2）

### 7.4 integrated

必填：
- `principal`（可选由系统自动推断）

可选：
- `realm`
- `kdc`
- `service_name`

适用：SQL Server / PostgreSQL / Oracle（Phase-2）

### 7.5 file_key

必填：
- `key_ref` 或 `wallet_ref`

适用：Oracle Wallet / SQLite SQLCipher（Phase-2）

### 7.6 no_auth

无认证字段；仅要求网络或文件参数完整。

## 8. 前端动态联动规则

1. 选中 `db_type` 后：
- 拉取该类型支持的 `auth_modes`
- 自动隐藏不适用字段

2. 切换 `auth_mode` 时：
- 清理上一个模式的敏感字段
- 重新做必填校验

3. 选中 `tls.enabled=true` 时：
- 强制校验 `ssl_mode`
- `verify_ca/verify_full` 下必须提供 `ca_cert`

4. 选中 `network.mode=ssh_tunnel` 时：
- 增加 SSH 认证分组（password/private_key）
- 连接测试先测 SSH，再测 DB

## 9. 安全基线（MVP）

1. 所有敏感字段仅在请求阶段明文出现，入库前加密
2. API 回包永不返回明文密码/token/key
3. 测试连接接口默认不打印完整连接串
4. 错误日志默认脱敏（用户名可保留，密码等全部屏蔽）
