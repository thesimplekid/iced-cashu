use std::sync::Arc;
use std::time::Duration;

use cdk::amount::{Amount, SplitTarget};
use cdk::nuts::CurrencyUnit;
use cdk::wallet::Wallet;
use cdk::UncheckedUrl;
use cdk_sqlite::WalletSQLiteDatabase;
use config::{data_dir, generate_mnemonic, get_seed, save_seed};
use iced::widget::{button, center, column, qr_code, row, text, text_input};
use iced::{clipboard, Alignment, Element, Task, Theme};

mod config;

const DEFAULT_MINT: &str = "https://mint.thesimplekid.dev";

pub fn main() -> iced::Result {
    iced::program("Cashu Wallet - Iced", IcedCashu::update, IcedCashu::view)
        .theme(IcedCashu::theme)
        .run()
}

#[derive(Default)]
struct IcedCashu {
    wallet: Option<Arc<Wallet>>,
    data: String,
    pay_invoice: String,
    invoice: String,
    token: String,
    qr_code: Option<qr_code::Data>,
    view: View,
    balance: u64,
    receive_amount: String,
    send_amount: String,
    active_mint: UncheckedUrl,
}

#[derive(Debug, Clone, Default)]
enum View {
    #[default]
    Main,
    Receive,
    Pay,
    Invoice,
    Token,
}

#[derive(Debug, Clone)]
enum Message {
    DataChanged(String),
    ReceiveDataChanged(String),
    SendDataChanged(String),
    Pay,
    PayBolt11Change(String),
    PayInvoice,
    NewWallet,
    WalletCreated(Wallet),
    MintQuote((String, String)),
    ReceiveEcash,
    Receive,
    Minted(u64),
    CheckBalance(u64),
    Balance(u64),
    CreateInvoice,
    CreateToken,
    TokenCreated(String),
    CopyInvoice,
    CopyToken,
    Home,
}

async fn new_wallet() -> Wallet {
    let db_path = data_dir().join("./cashu_iced.sqlite");
    let localstore = WalletSQLiteDatabase::new(&db_path.to_string_lossy())
        .await
        .unwrap();
    localstore.migrate().await;

    let seed = match get_seed() {
        Some(seed) => seed,
        None => {
            let seed = generate_mnemonic().unwrap();

            save_seed(&seed.to_string());
            seed
        }
    };

    let wallet = Wallet::new(Arc::new(localstore), &seed.to_seed_normalized(""), vec![]);
    wallet
}

async fn mint_quote(wallet: Arc<Wallet>, mint_url: UncheckedUrl, amount: u64) -> (String, String) {
    let quote = wallet
        .mint_quote(mint_url, CurrencyUnit::Sat, Amount::from(amount))
        .await
        .unwrap();

    (quote.request, quote.id)
}

async fn mint(wallet: Arc<Wallet>, mint_url: UncheckedUrl, quote_id: String) -> u64 {
    let mut paid = false;

    while !paid {
        paid = wallet
            .mint_quote_status(mint_url.clone(), &quote_id)
            .await
            .unwrap()
            .paid;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
    let amount = wallet
        .mint(mint_url, &quote_id, SplitTarget::default(), None)
        .await
        .unwrap();

    amount.into()
}

async fn receive(wallet: Arc<Wallet>, token: String) -> u64 {
    let amount = wallet
        .receive(&token, &SplitTarget::default(), None)
        .await
        .unwrap();

    amount.into()
}

async fn create_token(wallet: Arc<Wallet>, mint_url: UncheckedUrl, amount: u64) -> String {
    let quote = wallet
        .send(
            &mint_url,
            CurrencyUnit::Sat,
            Amount::from(amount),
            None,
            None,
            &SplitTarget::None,
        )
        .await
        .unwrap();

    quote
}

async fn pay_invoice(wallet: Arc<Wallet>, mint_url: UncheckedUrl, bolt11: String) -> u64 {
    let quote = wallet
        .melt_quote(mint_url.clone(), CurrencyUnit::Sat, bolt11, None)
        .await
        .unwrap();

    let paid = wallet
        .melt(&mint_url, &quote.id, SplitTarget::None)
        .await
        .unwrap();

    println!("invoice paid: {}", paid.paid);

    0
}

async fn check_balance(wallet: Arc<Wallet>) -> u64 {
    let amount = wallet.unit_balance(CurrencyUnit::Sat).await.unwrap();

    amount.into()
}

impl IcedCashu {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::DataChanged(data) => {
                self.data = data;
                Task::none()
            }
            Message::ReceiveDataChanged(data) => {
                if let Ok(data) = data.parse() {
                    self.receive_amount = data;
                }
                Task::none()
            }
            Message::SendDataChanged(data) => {
                if let Ok(data) = data.parse() {
                    self.send_amount = data;
                }
                Task::none()
            }
            Message::NewWallet => {
                self.active_mint = UncheckedUrl::from(DEFAULT_MINT);
                Task::perform(new_wallet(), Message::WalletCreated)
            }
            Message::WalletCreated(wallet) => {
                self.wallet = Some(Arc::new(wallet));
                let wallet = self.wallet.clone().unwrap();
                Task::perform(check_balance(wallet), Message::Balance)
            }
            Message::MintQuote((request, quote_id)) => {
                self.qr_code = qr_code::Data::new(&request).ok();
                self.invoice = request;

                self.view = View::Invoice;
                let wallet = self.wallet.clone().unwrap();
                Task::perform(
                    mint(wallet, self.active_mint.clone(), quote_id),
                    Message::Minted,
                )
            }
            Message::Minted(_amount) => {
                self.view = View::Main;
                let wallet = self.wallet.clone().unwrap();
                Task::perform(check_balance(wallet), Message::Balance)
            }
            Message::ReceiveEcash => {
                self.view = View::Receive;
                Task::none()
            }
            Message::Receive => {
                let wallet = self.wallet.clone().unwrap();
                self.view = View::Main;
                self.data = "".to_string();
                Task::perform(receive(wallet, self.data.clone()), Message::CheckBalance)
            }
            Message::CreateInvoice => {
                let wallet = self.wallet.clone().unwrap();
                let amount: u64 = self.receive_amount.parse().unwrap();
                Task::perform(
                    mint_quote(wallet, self.active_mint.clone(), amount),
                    Message::MintQuote,
                )
            }
            Message::CheckBalance(_amount) => {
                let wallet = self.wallet.clone().unwrap();
                Task::perform(check_balance(wallet), Message::Balance)
            }
            Message::Balance(amount) => {
                self.balance = amount;
                Task::none()
            }
            Message::CopyInvoice => {
                // HACK: Copy isnt working so print to console
                println!("{}", self.invoice);
                clipboard::write::<String>(self.invoice.clone());
                Task::none()
            }
            Message::CopyToken => {
                // HACK: Copy isnt working so print to console
                println!("{}", self.token);
                clipboard::write::<String>(self.token.clone());
                Task::none()
            }
            Message::PayBolt11Change(data) => {
                self.pay_invoice = data;
                Task::none()
            }
            Message::PayInvoice => {
                let wallet = self.wallet.clone().unwrap();
                self.view = View::Main;
                Task::perform(
                    pay_invoice(wallet, self.active_mint.clone(), self.pay_invoice.clone()),
                    Message::CheckBalance,
                )
            }
            Message::Pay => {
                self.view = View::Pay;
                Task::none()
            }
            Message::CreateToken => {
                let wallet = self.wallet.clone().unwrap();
                let amount: u64 = self.send_amount.parse().unwrap();
                Task::perform(
                    create_token(wallet, self.active_mint.clone(), amount),
                    Message::TokenCreated,
                )
            }
            Message::TokenCreated(token) => {
                self.token = token;
                self.view = View::Token;
                Task::none()
            }
            Message::Home => {
                let wallet = self.wallet.clone().unwrap();
                self.data = "".to_string();
                self.pay_invoice = "".to_string();
                self.token = "".to_string();
                self.qr_code = None;
                self.receive_amount = "".to_string();
                self.send_amount = "".to_string();

                self.view = View::Main;
                Task::perform(check_balance(wallet), Message::Balance)
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let title = text("Cashu").size(70);

        /*
                let input = text_input("Type the data of your QR code here...", &self.data)
                    .on_input(Message::DataChanged)
                    .size(30)
                    .padding(15);

                let choose_theme = row![
                    text("Theme:"),
                    pick_list(Theme::ALL, Some(&self.theme), Message::ThemeChanged,)
                ]
                .spacing(10)
                .align_items(Alignment::Center);
        */
        let view = match self.wallet {
            Some(_) => match &self.view {
                View::Main => Some(column![center(column![
                    row![text(self.balance).size(50), text("sats").size(40)],
                    row![
                        column![button(text("Receive")).on_press(Message::ReceiveEcash)],
                        column![button(text("Send")).on_press(Message::Pay)]
                    ]
                ])]),
                View::Receive => Some(column![
                    row![text_input("Paste your token", &self.data)
                        .on_input(Message::DataChanged)
                        .padding(15)],
                    row![button(text("Claim")).on_press(Message::Receive)],
                    row![text_input("Amount (sats)", &self.receive_amount)
                        .on_input(Message::ReceiveDataChanged)],
                    row![button(text("Create Invoice")).on_press(Message::CreateInvoice)],
                    center(row![button(text("Home")).on_press(Message::Home)])
                ]),
                View::Pay => Some(column![
                    row![text(self.balance).size(50), text("sats").size(40)],
                    row![text_input("Paste bolt11 invoice", &self.pay_invoice)
                        .on_input(Message::PayBolt11Change)
                        .padding(15)],
                    row![button(text("Pay Invoice")).on_press(Message::PayInvoice)],
                    row![text_input("Amount (sats)", &self.send_amount)
                        .on_input(Message::SendDataChanged)],
                    row![button(text("Create Token")).on_press(Message::CreateToken)],
                    center(row![button(text("Home")).on_press(Message::Home)])
                ]),
                View::Invoice => Some(column![
                    row![self
                        .qr_code
                        .as_ref()
                        .map(|data| qr_code(data).cell_size(10))
                        .unwrap()],
                    row![button(text("Copy")).on_press(Message::CopyInvoice)],
                    row![button(text("Home")).on_press(Message::Home)]
                ]),
                View::Token => Some(column![
                    row![text(&self.token)],
                    row![button(text("Copy")).on_press(Message::CopyToken)],
                    row![button(text("Home")).on_press(Message::Home)]
                ]),
            },
            None => Some(column![
                button(text("New Wallet")).on_press(Message::NewWallet)
            ]),
        };

        let content = column![title]
            .push_maybe(view)
            .width(700)
            .spacing(20)
            .align_items(Alignment::Center);

        center(content).padding(20).into()
    }

    fn theme(&self) -> Theme {
        Theme::Dracula
    }
}
