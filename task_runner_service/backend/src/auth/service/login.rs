use super::*;

impl AuthService {
    pub async fn login(&self, username: &str, password: &str) -> Result<LoginResponse, String> {
        let username = normalize_username(username)?;
        let Some(mut user) = self.store.get_user_by_username(username.as_str()).await? else {
            return Err("用户名或密码错误".to_string());
        };
        if !user.enabled {
            return Err("用户已禁用".to_string());
        }
        if !verify_password(password, user.password_hash.as_str()) {
            return Err("用户名或密码错误".to_string());
        }

        user.last_login_at = Some(now_rfc3339());
        user.updated_at = now_rfc3339();
        let user = self.store.save_user(user).await?;
        let current = CurrentUser::from(&user);
        let token = self.create_session(current.clone(), None);
        Ok(LoginResponse {
            token,
            user: current.public_user(),
        })
    }

    pub async fn issue_agent_token(
        &self,
        username: &str,
        password: &str,
    ) -> Result<AgentTokenResponse, String> {
        let username = normalize_username(username)?;
        let Some(mut user) = self.store.get_user_by_username(username.as_str()).await? else {
            return Err("用户名或密码错误".to_string());
        };
        if !user.enabled {
            return Err("用户已禁用".to_string());
        }
        if user.role != UserRole::Agent {
            return Err("该接口仅允许 AI agent 账号换取 token".to_string());
        }
        if !verify_password(password, user.password_hash.as_str()) {
            return Err("用户名或密码错误".to_string());
        }

        user.last_login_at = Some(now_rfc3339());
        user.updated_at = now_rfc3339();
        let user = self.store.save_user(user).await?;
        let current = CurrentUser::from(&user);
        let token = self.create_session(current.clone(), Some(AGENT_TOKEN_TTL_SECONDS));
        Ok(AgentTokenResponse {
            token,
            token_type: "Bearer".to_string(),
            expires_in: AGENT_TOKEN_TTL_SECONDS,
            user: current.public_user(),
        })
    }
}
