use crate::{parser, AppState, ProcessorMessage};
use anyhow::{anyhow, Result};
use dotenv::dotenv;
use futures::task::SpawnExt;
use log::{error, info};
use reqwest::Client;
use serde_json::Value;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::{Arc};
use teloxide::dispatching::Dispatcher;
use teloxide::dispatching::HandlerExt;
use teloxide::dispatching::UpdateFilterExt;
use teloxide::types::ChatId;
use teloxide::utils::command::CommandDescriptions;
use teloxide::{dispatching, prelude::*, utils::command::BotCommands};
use tokio::sync::watch::Receiver;
use tokio::sync::{watch, Mutex, mpsc};
use tokio::time::{self, Duration};

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
enum Command {
    #[command(aliases = ["S", "s"])]
    Stats,
    #[command(aliases = ["S", "s"])]
    Run,
    #[command(aliases = ["K", "k"])]
    Keys,
    #[command(aliases = ["R", "r"])]
    Reboot,
    #[command(aliases = ["h", "?"])]
    Help,
}

async fn run_parser(
    parser_state: Arc<Mutex<AppState>>,
    shutdown_tx: Receiver<bool>,
    rx: mpsc::Sender<ProcessorMessage>,
) {
    tokio::spawn(async { parser::run_parser(parser_state, shutdown_tx, rx).await });
}

async fn handle_command(
    bot: Bot,
    message: Message,
    command: Command,
    state: Arc<Mutex<AppState>>,
    rx: Receiver<bool>,
    parser_tx: mpsc::Sender<ProcessorMessage>,
) -> ResponseResult<()> {
    match command {
        Command::Stats => {
            let app_state = state.lock().await;
            let curr_bloc = get_current_block_height().await.unwrap_or(0);
            let response = format!(
                "📊 **Текущая статистика**\n\n\
                    🛠 Текущий обрабатываемый блок: {}\n\
                    🔢 Текущий блок explorer: {}",
                app_state.current_block, curr_bloc
            );
            if let Err(e) = bot
                .send_message(message.chat.id, response)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .await
            {
                error!("Ошибка при отправке статистики: {:?}", e);
            }
        }
        Command::Keys => {
            let path_to_file = env::var("FILE_PATH").unwrap_or("./keys.txt".parse().unwrap());
            let file = File::open(path_to_file).unwrap();
            let reader = BufReader::new(file);
            let keys = reader
                .lines()
                .filter_map(|line| line.ok())
                .collect::<Vec<String>>()
                .join("\n");
            let response = format!("🔑 **Содержимое `keys.txt`:**\n\n```\n{}\n```", keys);
            if let Err(e) = bot
                .send_message(message.chat.id, response)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .await
            {
                error!("Ошибка при отправке ключей: {:?}", e);
            }
        }
        Command::Reboot => {
            let response = "🛠 **Обновляю актуальный блок...".to_string();
            if let Err(e) = bot
                .send_message(message.chat.id, response.to_string())
                .await
            {
                error!("Ошибка при отправке сообщения: {:?}", e);
            }
            let mut app_state = state.lock().await;
            let curr_block = get_current_block_height().await.unwrap();
            let diff = curr_block - app_state.current_block;
            app_state.current_block = curr_block;

            let diff = format!("🛠 **Отставание на {diff} блок сокращено");

            if let Err(e) = bot.send_message(message.chat.id, diff).await {
                error!("Ошибка при отправке сообщения: {:?}", e);
            }
        }
        Command::Help => {
            let response: CommandDescriptions = Command::descriptions();
            if let Err(e) = bot
                .send_message(message.chat.id, response.to_string())
                .await
            {
                error!("Ошибка при отправке помощи: {:?}", e);
            }
        }
        Command::Run => {
            let mut app_state = state.lock().await;
            let curr_block = get_current_block_height().await.unwrap();
            app_state.current_block = curr_block;
            let response = "🛠 **Запускаю сервер".to_string();
            if let Err(e) = bot
                .send_message(message.chat.id, response.to_string())
                .await
            {
                error!("Ошибка при отправке сообщения: {:?}", e);
            }
            run_parser(Arc::clone(&state), rx, parser_tx).await;
        }
    };
    Ok(())
}

pub async fn run_bot(current_block: u64) {
    dotenv().ok();

    let bot_token = env::var("TELOXIDE_TOKEN").expect("TELOXIDE_TOKEN не установлен");
    let chat_id: i64 = env::var("CHAT_ID")
        .expect("CHAT_ID не установлен")
        .parse()
        .expect("CHAT_ID должен быть числом");
    let file_path = env::var("FILE_PATH").expect("FILE_PATH не установлен");

    let bot = Bot::new(bot_token);
    let chat = ChatId(chat_id);
    let state = Arc::new(Mutex::new(AppState::new(current_block)));

    match env::current_dir() {
        Ok(path) => println!("Текущая рабочая директория: {}", path.display()),
        Err(e) => eprintln!("Не удалось получить текущую рабочую директорию: {:?}", e),
    }

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let (tx, mut rx) = mpsc::channel::<ProcessorMessage>(100);
    let bot_clone2 = bot.clone();
    let chat_clone2 = chat.clone();

    let parser_tx = tx.clone();
    let commands = tokio::spawn(async move {
        let handler = Update::filter_message().branch(
            dptree::entry()
                .filter_command::<Command>()
                .endpoint(handle_command),
        );

        let mut dispatcher = Dispatcher::builder(bot, handler)
            .dependencies(dptree::deps![
                state.clone(),
                shutdown_rx.clone(),
                parser_tx.clone()
            ])
            .enable_ctrlc_handler()
            .build();
        dispatcher.dispatch().await;

        if let Err(e) = shutdown_tx.send(true) {
            error!("Не удалось отправить сигнал завершения: {:?}", e);
        }
    });

    println!("Используемый путь к файлу: {}", file_path);

    let log = tokio::spawn(async move {
        while let Some(received) = rx.recv().await {
            match received {
                ProcessorMessage::Success(msg) => {
                    let response = format!(
                        "🔑 **Ключ поменян c  `{:?}`\n на `{:?}`\n для адреса `{:?}`",
                        msg.old_key, msg.new_key, msg.old_address
                    );
                    match bot_clone2.send_message(chat_clone2, response).await {
                        Ok(_) => println!("Сообщение успешно отправлено."),
                        Err(e) => eprintln!("Ошибка при отправке сообщения {e:?}"),
                    }
                }
                ProcessorMessage::Progress(msg) => {
                    let response = format!("🔑 **Пытаюсь поменять ключ: `{:?}`", msg);
                    match bot_clone2.send_message(chat_clone2, response).await {
                        Ok(_) => println!("Сообщение успешно отправлено."),
                        Err(e) => eprintln!("Ошибка при отправке сообщения"),
                    }
                }
                ProcessorMessage::Error(msg) => {
                    let response = format!(
                        "🔑 **Не удалось поменять ключ: `{:?}` - {:?}",
                        msg.old_key, msg.err
                    );
                    match bot_clone2.send_message(chat_clone2, response).await {
                        Ok(_) => println!("Сообщение успешно отправлено."),
                        Err(e) => eprintln!("Ошибка при отправке сообщения"),
                    }
                }
                ProcessorMessage::TryButFailed(msg) => {
                    let response = format!(
                        "🔑 **Возможно нехватило средств c `{:?}` \n на `{:?}`\n для адреса `{:?}`, но переперьверить",
                        msg.old_key, msg.new_key, msg.old_address
                    );
                    match bot_clone2.send_message(chat_clone2, response).await {
                        Ok(_) => println!("Сообщение успешно отправлено."),
                        Err(e) => eprintln!("Ошибка при отправке сообщения {e:?}"),
                    }
                }
            }
        }
    });
    tokio::join!(commands, log);
}

fn get_initial_line_count(path: &str) -> Result<usize> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let count = reader.lines().count();
    Ok(count)
}

fn read_new_lines(path: &str, sent_lines: usize) -> Result<Vec<String>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            if idx >= sent_lines {
                match line {
                    Ok(content) => Some(content),
                    Err(_) => None,
                }
            } else {
                None
            }
        })
        .collect();
    Ok(lines)
}

fn get_saved_line_count(path: &str) -> Result<usize> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut content = String::new();
    reader.read_line(&mut content)?;
    let count = content
        .trim()
        .parse()
        .map_err(|_| anyhow!("Неверный формат числа в файле состояния"))?;
    Ok(count)
}

async fn get_current_block_height() -> Option<u64> {
    let client = Arc::new(
        Client::builder()
            .pool_idle_timeout(None)
            .pool_max_idle_per_host(100)
            .build()
            .expect("Не удалось создать клиент"),
    );
    let url = "https://api.mainnet.aptoslabs.com/v1/".to_string();

    if let Ok(response) = client.get(&url).send().await {
        if let Ok(body) = response.text().await {
            if let Ok(data) = serde_json::from_str::<Value>(&body) {
                return data
                    .get("block_height")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok());
            }
        }
    }
    None
}
