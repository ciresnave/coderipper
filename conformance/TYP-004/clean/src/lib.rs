pub struct Email(String);

impl Email {
    pub fn parse(text: &str) -> Option<Email> {
        text.contains('@').then(|| Email(text.to_string()))
    }
}

pub fn send_welcome(email: &Email) -> usize {
    email.0.len()
}
