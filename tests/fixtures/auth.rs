pub struct User {
    pub id: String,
    pub token: String,
}

pub async fn authenticate_with_oauth(token: &str) -> Option<User> {
    if token.trim().is_empty() {
        return None;
    }

    Some(User {
        id: "demo-user".to_string(),
        token: token.to_string(),
    })
}
