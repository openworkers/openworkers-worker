//! The `send_email` binding.
//!
//! The types are here so that a workers-rs application that announces by
//! email still compiles. Sending is not: OpenWorkers has no email operation
//! yet, so every send reports that rather than dropping the message.

use crate::env::EnvBinding;
use crate::Env;
use crate::Error;
use crate::Result;

const UNSUPPORTED: &str = "send_email is not supported by this platform yet";

impl Env {
    /// An email binding. The lookup succeeds; sending is what fails.
    pub fn send_email(&self, binding: &str) -> Result<SendEmail> {
        self.get_binding(binding)
    }
}

/// A `send_email` binding.
#[derive(Debug, Clone)]
pub struct SendEmail;

impl EnvBinding for SendEmail {
    const TYPE_NAME: &'static str = "SendEmail";

    fn get(_name: &str) -> Result<Self> {
        Ok(SendEmail)
    }
}

impl SendEmail {
    pub async fn send(&self, _message: &EmailMessage) -> Result<EmailSendResult> {
        Err(Error::RustError(UNSUPPORTED.into()))
    }

    pub async fn send_with_builder(&self, _builder: &SendEmailBuilder) -> Result<EmailSendResult> {
        Err(Error::RustError(UNSUPPORTED.into()))
    }
}

/// What a successful send would have reported.
#[derive(Debug, Clone)]
pub struct EmailSendResult {
    message_id: String,
}

impl EmailSendResult {
    pub fn new(message_id: &str) -> Self {
        EmailSendResult {
            message_id: message_id.to_string(),
        }
    }

    pub fn message_id(&self) -> String {
        self.message_id.clone()
    }
}

/// A named sender or recipient.
#[derive(Debug, Clone, Default)]
pub struct EmailAddress {
    name: String,
    email: String,
}

impl EmailAddress {
    pub fn new(name: &str, email: &str) -> Self {
        EmailAddress {
            name: name.to_string(),
            email: email.to_string(),
        }
    }

    pub fn name(&self) -> String {
        self.name.clone()
    }

    pub fn set_name(&mut self, value: &str) {
        self.name = value.to_string();
    }

    pub fn email(&self) -> String {
        self.email.clone()
    }

    pub fn set_email(&mut self, value: &str) {
        self.email = value.to_string();
    }
}

/// A raw RFC 5322 message, as `cloudflare:email` takes one.
#[derive(Debug, Clone)]
pub struct EmailMessage {
    pub from: String,
    pub to: String,
    pub raw: String,
}

impl EmailMessage {
    pub fn new(from: &str, to: &str, raw: &str) -> Result<Self> {
        Ok(EmailMessage {
            from: from.to_string(),
            to: to.to_string(),
            raw: raw.to_string(),
        })
    }

    pub fn from(&self) -> String {
        self.from.clone()
    }

    pub fn to(&self) -> String {
        self.to.clone()
    }
}

/// A structured message, built rather than assembled by hand.
#[derive(Debug, Clone, Default)]
pub struct SendEmailBuilder {
    pub from: String,
    pub from_name: Option<String>,
    pub to: Vec<String>,
    pub subject: String,
    pub reply_to: Option<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub text: Option<String>,
    pub html: Option<String>,
}

impl SendEmailBuilder {
    pub fn new(from: &str, to: &str, subject: &str) -> Self {
        SendEmailBuilder::builder(from, to, subject).build()
    }

    pub fn new_with_str_and_slice(from: &str, to: &[String], subject: &str) -> Self {
        SendEmailBuilder::builder_with_str_and_slice(from, to, subject).build()
    }

    pub fn new_with_email_address_and_str(from: &EmailAddress, to: &str, subject: &str) -> Self {
        SendEmailBuilder::builder_with_email_address_and_str(from, to, subject).build()
    }

    pub fn builder(from: &str, to: &str, subject: &str) -> SendEmailBuilderBuilder {
        SendEmailBuilderBuilder {
            inner: SendEmailBuilder {
                from: from.to_string(),
                to: vec![to.to_string()],
                subject: subject.to_string(),
                ..SendEmailBuilder::default()
            },
        }
    }

    pub fn builder_with_str_and_slice(
        from: &str,
        to: &[String],
        subject: &str,
    ) -> SendEmailBuilderBuilder {
        SendEmailBuilderBuilder {
            inner: SendEmailBuilder {
                from: from.to_string(),
                to: to.to_vec(),
                subject: subject.to_string(),
                ..SendEmailBuilder::default()
            },
        }
    }

    pub fn builder_with_email_address_and_str(
        from: &EmailAddress,
        to: &str,
        subject: &str,
    ) -> SendEmailBuilderBuilder {
        SendEmailBuilderBuilder {
            inner: SendEmailBuilder {
                from: from.email(),
                from_name: Some(from.name()),
                to: vec![to.to_string()],
                subject: subject.to_string(),
                ..SendEmailBuilder::default()
            },
        }
    }

    pub fn builder_with_email_address_and_slice(
        from: &EmailAddress,
        to: &[String],
        subject: &str,
    ) -> SendEmailBuilderBuilder {
        SendEmailBuilderBuilder {
            inner: SendEmailBuilder {
                from: from.email(),
                from_name: Some(from.name()),
                to: to.to_vec(),
                subject: subject.to_string(),
                ..SendEmailBuilder::default()
            },
        }
    }
}

/// The builder for [`SendEmailBuilder`], named as workers-rs names it.
#[derive(Debug, Clone, Default)]
pub struct SendEmailBuilderBuilder {
    inner: SendEmailBuilder,
}

impl SendEmailBuilderBuilder {
    pub fn reply_to(mut self, value: &str) -> Self {
        self.inner.reply_to = Some(value.to_string());
        self
    }

    pub fn reply_to_with_email_address(mut self, value: &EmailAddress) -> Self {
        self.inner.reply_to = Some(value.email());
        self
    }

    pub fn cc(mut self, value: &str) -> Self {
        self.inner.cc = vec![value.to_string()];
        self
    }

    pub fn cc_with_slice(mut self, value: &[String]) -> Self {
        self.inner.cc = value.to_vec();
        self
    }

    pub fn bcc(mut self, value: &str) -> Self {
        self.inner.bcc = vec![value.to_string()];
        self
    }

    pub fn bcc_with_slice(mut self, value: &[String]) -> Self {
        self.inner.bcc = value.to_vec();
        self
    }

    pub fn text(mut self, value: &str) -> Self {
        self.inner.text = Some(value.to_string());
        self
    }

    pub fn html(mut self, value: &str) -> Self {
        self.inner.html = Some(value.to_string());
        self
    }

    pub fn build(self) -> SendEmailBuilder {
        self.inner
    }
}
