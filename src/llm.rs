use color_eyre::eyre::Result;
use genai::Client;
use genai::chat::printer::{PrintChatStreamOptions, print_chat_stream};
use genai::chat::{ChatMessage, ChatRequest};

#[tokio::main]
pub(crate) async fn chat() -> Result<()> {
    color_eyre::install()?;

    const MODEL_ZAI: &str = "zai-coding::glm-5v-turbo";

    const MODEL_AND_KEY_ENV_NAME_LIST: &[(&str, &str)] = &[
        // -- De/activate models/providers
        (MODEL_ZAI, "ZAI_API_KEY"),
    ];
    let question = "Why is the sky red?";

    let chat_req = ChatRequest::new(vec![
        // -- Messages (de/activate to see the differences)
        ChatMessage::system("Answer in one sentence"),
        ChatMessage::user(question),
    ]);

    let client = Client::default();

    let print_options = PrintChatStreamOptions::from_print_events(false);

    for (model, env_name) in MODEL_AND_KEY_ENV_NAME_LIST {
        // Skip if the environment name is not set
        if !env_name.is_empty() && std::env::var(env_name).is_err() {
            println!("===== Skipping model: {model} (env var not set: {env_name})");
            continue;
        }

        let adapter_kind = client
            .resolve_service_target(model)
            .await?
            .model
            .adapter_kind;

        println!("\n===== MODEL: {model} ({adapter_kind}) =====");

        println!("\n--- Question:\n{question}");

        println!("\n--- Answer:");
        let chat_res = client.exec_chat(model, chat_req.clone(), None).await?;
        println!("{}", chat_res.first_text().unwrap_or("NO ANSWER"));

        println!("\n--- Answer: (streaming)");
        let chat_res = client
            .exec_chat_stream(model, chat_req.clone(), None)
            .await?;
        print_chat_stream(chat_res, Some(&print_options)).await?;

        println!();
    }

    Ok(())
}
