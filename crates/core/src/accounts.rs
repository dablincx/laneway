//! Accounts: first-launch setup, login, sessions.

use std::net::IpAddr;

use chrono::{Duration, Utc};
use laneway_auth::PasswordProblem;
use laneway_db::PgPool;
use laneway_db::users::{NewUser, User};
use uuid::Uuid;

use crate::Error;

/// How long a login session lasts.
pub const SESSION_DAYS: i64 = 30;

/// Input that failed validation, by field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invalid {
    Email,
    Username,
    Name,
    PasswordTooShort,
    PasswordTooLong,
    BoardName,
    ColumnName,
    CardTitle,
    Description,
    WipLimit,
    Position,
}

/// Account details for a new user.
#[derive(Debug, Clone)]
pub struct NewAccount {
    pub email: String,
    pub username: String,
    pub name: String,
    pub password: String,
}

impl NewAccount {
    /// Trims and checks every field.
    fn validated(&self) -> Result<(String, String, String), Error> {
        let email = self.email.trim().to_owned();
        let username = self.username.trim().to_owned();
        let name = self.name.trim().to_owned();
        if !valid_email(&email) {
            return Err(Error::Invalid(Invalid::Email));
        }
        if !valid_username(&username) {
            return Err(Error::Invalid(Invalid::Username));
        }
        if name.is_empty() || name.chars().count() > 100 {
            return Err(Error::Invalid(Invalid::Name));
        }
        laneway_auth::check_new_password(&self.password).map_err(|p| {
            Error::Invalid(match p {
                PasswordProblem::TooShort => Invalid::PasswordTooShort,
                PasswordProblem::TooLong => Invalid::PasswordTooLong,
            })
        })?;
        Ok((email, username, name))
    }
}

/// Loose check: something@something.something, no spaces, at most 254 chars.
/// The real check is the verification email (v0.3).
fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.rsplit_once('@') else { return false };
    email.len() <= 254
        && !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !email.chars().any(char::is_whitespace)
}

/// 2-32 chars of `a-z 0-9 _ . -`, starting with a letter or digit. Stored as typed, unique
/// ignoring case.
fn valid_username(username: &str) -> bool {
    let len = username.chars().count();
    (2..=32).contains(&len)
        && username.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && username.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// `true` while no admin exists, so the instance still needs `/setup`.
pub async fn needs_setup(pool: &PgPool) -> Result<bool, Error> {
    Ok(!laneway_db::users::admin_exists(pool).await?)
}

/// While no admin exists, creates a fresh one-time setup token and returns it for the logs.
/// A restart replaces it, so only the newest printed token works.
pub async fn issue_setup_token(pool: &PgPool) -> Result<Option<String>, Error> {
    if !needs_setup(pool).await? {
        laneway_db::setup::clear(pool).await?;
        return Ok(None);
    }
    let token = laneway_auth::new_token()?;
    laneway_db::setup::replace_token(pool, &token.hash).await?;
    Ok(Some(token.token))
}

/// Creates the first admin with the setup token from the logs.
pub async fn complete_setup(pool: &PgPool, setup_token: &str, account: &NewAccount) -> Result<User, Error> {
    let (email, username, name) = account.validated()?;
    let mut tx = pool.begin().await?;
    // Serializes concurrent setup attempts.
    sqlx::query!("LOCK TABLE setup_tokens IN EXCLUSIVE MODE").execute(&mut *tx).await?;
    if laneway_db::users::admin_exists(&mut *tx).await? {
        return Err(Error::SetupDone);
    }
    if !laneway_db::setup::consume_token(&mut *tx, &laneway_auth::hash_token(setup_token.trim())).await? {
        return Err(Error::InvalidSetupToken);
    }
    let user = create_user(&mut tx, &email, &username, &name, &account.password, "admin").await?;
    tx.commit().await?;
    tracing::info!(user = %user.id, "first admin created");
    Ok(user)
}

/// Creates the admin from `LANEWAY_INITIAL_ADMIN_*` env vars if no admin exists yet.
/// Returns the user when one was created.
pub async fn ensure_initial_admin(pool: &PgPool, account: &NewAccount) -> Result<Option<User>, Error> {
    let (email, username, name) = account.validated()?;
    let mut tx = pool.begin().await?;
    sqlx::query!("LOCK TABLE setup_tokens IN EXCLUSIVE MODE").execute(&mut *tx).await?;
    if laneway_db::users::admin_exists(&mut *tx).await? {
        return Ok(None);
    }
    let user = create_user(&mut tx, &email, &username, &name, &account.password, "admin").await?;
    laneway_db::setup::clear(&mut *tx).await?;
    tx.commit().await?;
    tracing::info!(user = %user.id, "initial admin created from env");
    Ok(Some(user))
}

async fn create_user(
    tx: &mut sqlx::PgTransaction<'_>,
    email: &str,
    username: &str,
    name: &str,
    password: &str,
    role: &str,
) -> Result<User, Error> {
    let hash = laneway_auth::hash_password(password)?;
    let new = NewUser { email, username, name, instance_role: role, password_hash: Some(&hash) };
    match laneway_db::users::create(&mut **tx, &new).await {
        Ok(user) => {
            laneway_db::workspaces::create_personal(&mut **tx, user.id, &user.name).await?;
            Ok(user)
        }
        Err(sqlx::Error::Database(e)) if e.constraint() == Some("users_email_uidx") => Err(Error::EmailTaken),
        Err(sqlx::Error::Database(e)) if e.constraint() == Some("users_username_uidx") => {
            Err(Error::UsernameTaken)
        }
        Err(e) => Err(e.into()),
    }
}

/// Where a login came from, shown later in the sessions list.
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub user_agent: Option<String>,
    pub ip: Option<IpAddr>,
}

/// A new session. `token` goes into the cookie and is never stored.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: Uuid,
    pub token: String,
    pub user: User,
    pub expires_at: chrono::DateTime<Utc>,
}

/// Checks email-or-username + password and starts a session.
/// Unknown user, wrong password and disabled account all give the same error.
pub async fn login(
    pool: &PgPool,
    login: &str,
    password: &str,
    client: &ClientInfo,
) -> Result<Session, Error> {
    let found = laneway_db::users::find_for_login(pool, login.trim()).await?;
    let user = match found {
        Some((user, Some(hash))) if laneway_auth::verify_password(password, &hash) && !user.disabled => user,
        Some((_, Some(_))) => return Err(Error::InvalidCredentials),
        _ => {
            laneway_auth::dummy_verify(password);
            return Err(Error::InvalidCredentials);
        }
    };
    start_session(pool, user, client).await
}

/// Starts a session for a user who just proved who they are.
pub async fn start_session(pool: &PgPool, user: User, client: &ClientInfo) -> Result<Session, Error> {
    let token = laneway_auth::new_token()?;
    let expires_at = Utc::now() + Duration::days(SESSION_DAYS);
    let user_agent = client.user_agent.as_deref().map(|ua| truncate(ua, 512));
    let id = laneway_db::sessions::create(
        pool,
        &laneway_db::sessions::NewSession {
            user_id: user.id,
            token_hash: &token.hash,
            user_agent: user_agent.as_deref(),
            ip: client.ip,
            expires_at,
        },
    )
    .await?;
    Ok(Session { id, token: token.token, user, expires_at })
}

/// The user behind a session token, if the session is live.
pub async fn authenticate(pool: &PgPool, token: &str) -> Result<Option<(Uuid, User)>, Error> {
    Ok(laneway_db::sessions::user_for_token(pool, &laneway_auth::hash_token(token)).await?)
}

pub async fn logout(pool: &PgPool, token: &str) -> Result<(), Error> {
    laneway_db::sessions::delete_by_token(pool, &laneway_auth::hash_token(token)).await?;
    Ok(())
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// Test helper: inserts a non-admin user with a personal workspace.
#[cfg(test)]
pub(crate) mod tests_support {
    use super::*;

    pub async fn create_plain_user(tx: &mut sqlx::PgTransaction<'_>, account: &NewAccount) -> Uuid {
        create_user(tx, &account.email, &account.username, &account.name, &account.password, "user")
            .await
            .unwrap()
            .id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account(email: &str, username: &str) -> NewAccount {
        NewAccount {
            email: email.into(),
            username: username.into(),
            name: "Test User".into(),
            password: "a long enough password".into(),
        }
    }

    #[test]
    fn email_and_username_rules() {
        for ok in ["a@b.co", "first.last+tag@example.com"] {
            assert!(valid_email(ok), "{ok}");
        }
        for bad in ["", "no-at", "@example.com", "a@nodot", "a b@c.de", "a@.com", "a@com."] {
            assert!(!valid_email(bad), "{bad}");
        }
        for ok in ["al", "alex_99", "a.b-c", "Alex"] {
            assert!(valid_username(ok), "{ok}");
        }
        for bad in ["a", "_alex", "al ex", "alex!", &"x".repeat(33)] {
            assert!(!valid_username(bad), "{bad}");
        }
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn setup_flow(pool: PgPool) {
        assert!(needs_setup(&pool).await.unwrap());
        let token = issue_setup_token(&pool).await.unwrap().unwrap();

        let wrong = complete_setup(&pool, "nope", &account("admin@example.com", "admin")).await;
        assert!(matches!(wrong, Err(Error::InvalidSetupToken)));

        let admin = complete_setup(&pool, &token, &account("admin@example.com", "admin")).await.unwrap();
        assert!(admin.is_admin());
        assert!(!needs_setup(&pool).await.unwrap());

        // The token is single use, and setup is closed now.
        let again = complete_setup(&pool, &token, &account("b@example.com", "b2")).await;
        assert!(matches!(again, Err(Error::SetupDone)));
        assert!(issue_setup_token(&pool).await.unwrap().is_none());
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn a_restart_replaces_the_setup_token(pool: PgPool) {
        let old = issue_setup_token(&pool).await.unwrap().unwrap();
        let new = issue_setup_token(&pool).await.unwrap().unwrap();
        let result = complete_setup(&pool, &old, &account("a@example.com", "aa")).await;
        assert!(matches!(result, Err(Error::InvalidSetupToken)));
        complete_setup(&pool, &new, &account("a@example.com", "aa")).await.unwrap();
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn setup_validates_input(pool: PgPool) {
        let token = issue_setup_token(&pool).await.unwrap().unwrap();
        let mut bad = account("admin@example.com", "admin");
        bad.password = "short".into();
        let result = complete_setup(&pool, &token, &bad).await;
        assert!(matches!(result, Err(Error::Invalid(Invalid::PasswordTooShort))));
        // A validation error doesn't burn the token.
        complete_setup(&pool, &token, &account("admin@example.com", "admin")).await.unwrap();
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn initial_admin_from_env_only_once(pool: PgPool) {
        let created = ensure_initial_admin(&pool, &account("env@example.com", "env")).await.unwrap();
        assert!(created.is_some());
        let again = ensure_initial_admin(&pool, &account("other@example.com", "other")).await.unwrap();
        assert!(again.is_none());
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn login_with_email_or_username_any_case(pool: PgPool) {
        ensure_initial_admin(&pool, &account("Admin@Example.com", "Admin")).await.unwrap();
        let client = ClientInfo::default();
        for login_name in ["admin@example.com", "ADMIN", "Admin"] {
            let session = login(&pool, login_name, "a long enough password", &client).await.unwrap();
            let (_, user) = authenticate(&pool, &session.token).await.unwrap().unwrap();
            assert_eq!(user.username, "Admin");
        }
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn wrong_password_and_unknown_user_look_the_same(pool: PgPool) {
        ensure_initial_admin(&pool, &account("admin@example.com", "admin")).await.unwrap();
        let client = ClientInfo::default();
        assert!(matches!(
            login(&pool, "admin", "wrong password!!", &client).await,
            Err(Error::InvalidCredentials)
        ));
        assert!(matches!(
            login(&pool, "nobody", "whatever password", &client).await,
            Err(Error::InvalidCredentials)
        ));
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn disabled_users_and_expired_sessions_are_rejected(pool: PgPool) {
        let admin =
            ensure_initial_admin(&pool, &account("admin@example.com", "admin")).await.unwrap().unwrap();
        let session = login(&pool, "admin", "a long enough password", &ClientInfo::default()).await.unwrap();

        sqlx::query!("UPDATE sessions SET expires_at = now() - interval '1 second'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(authenticate(&pool, &session.token).await.unwrap().is_none());

        let session = login(&pool, "admin", "a long enough password", &ClientInfo::default()).await.unwrap();
        sqlx::query!("UPDATE users SET disabled = true WHERE id = $1", admin.id)
            .execute(&pool)
            .await
            .unwrap();
        assert!(authenticate(&pool, &session.token).await.unwrap().is_none());
        let result = login(&pool, "admin", "a long enough password", &ClientInfo::default()).await;
        assert!(matches!(result, Err(Error::InvalidCredentials)));
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn logout_ends_the_session(pool: PgPool) {
        ensure_initial_admin(&pool, &account("admin@example.com", "admin")).await.unwrap();
        let session = login(&pool, "admin", "a long enough password", &ClientInfo::default()).await.unwrap();
        logout(&pool, &session.token).await.unwrap();
        assert!(authenticate(&pool, &session.token).await.unwrap().is_none());
    }

    #[sqlx::test(migrator = "laneway_db::MIGRATOR")]
    async fn duplicate_email_or_username(pool: PgPool) {
        ensure_initial_admin(&pool, &account("admin@example.com", "admin")).await.unwrap();
        let mut tx = pool.begin().await.unwrap();
        let taken =
            create_user(&mut tx, "ADMIN@example.com", "other", "x", "a long enough password", "user").await;
        assert!(matches!(taken, Err(Error::EmailTaken)));
        let mut tx = pool.begin().await.unwrap();
        let taken =
            create_user(&mut tx, "new@example.com", "ADMIN", "x", "a long enough password", "user").await;
        assert!(matches!(taken, Err(Error::UsernameTaken)));
    }
}
