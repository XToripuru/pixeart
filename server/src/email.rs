use lettre::{
    transport::smtp::authentication::Credentials, AsyncSmtpTransport, AsyncTransport, Message,
    Tokio1Executor,
};
use std::{error::Error, sync::Arc, };
use tokio::{sync::Mutex, spawn};

#[derive(Clone)]
pub struct EmailHandler {
    smtp: Arc<AsyncSmtpTransport<Tokio1Executor>>,
}

impl EmailHandler {
    pub fn init() -> Self {
        let credentials =
            Credentials::new("support@pixeart.online".to_owned(), shared::secret::MAIL_PASSWORD.to_owned());
        let smtp = AsyncSmtpTransport::<Tokio1Executor>::relay(shared::secret::MAIL_SERVER_ADDR)
            .unwrap()
            .credentials(credentials)
            .port(465)
            .build();

        EmailHandler {
            smtp: Arc::new(smtp),
        }
    }
    pub async fn verify(&self, email: &str, url: &str) -> Result<(), Box<dyn Error>> {
        let msg = Message::builder()
            .from("PixeArt <support@pixeart.online>".parse()?)
            .to(format!("<{email}>").parse()?)
            .subject("Account verification")
            .body(format!("Click to verify account\n{url}"))?;

        //self.logs.lock().await.log(format!("Veryfing - {}", email)).await;
        self.send_mail(msg);

        Ok(())
    }
    pub async fn pswd_reset(&self, email: &str, url: &str) -> Result<(), Box<dyn Error>> {
        let msg = Message::builder()
            .from("PixeArt <support@pixeart.online>".parse()?)
            .to(format!("<{email}>").parse()?)
            .subject("Password reset")
            .body(format!("Click to reset password\n{url}"))?;

        //self.logs.lock().await.log(format!("Recovering - {}", email)).await;
        self.send_mail(msg);

        Ok(())
    }
    fn send_mail(&self, msg: Message) {
        let smtp = Arc::clone(&self.smtp);

        spawn(async move {
            let res = smtp.send(msg).await;
        });
    }
}
