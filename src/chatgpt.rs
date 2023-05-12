use reqwest::{
    header::{self, HeaderMap, HeaderValue},
    Client,
};

#[derive(Debug, serde::Serialize)]
struct Request {
    model: Model,
    messages: Vec<Message>,
}

#[derive(Debug, serde::Serialize)]
enum Model {
    #[serde(rename = "gpt-3.5-turbo")]
    Gpt35Turbo,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Message {
    role: Role,
    content: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum Response {
    Error { error: Error },
    Success(SuccessResponse),
}

#[derive(Debug, serde::Deserialize)]
struct Error {
    message: String,
    // type: String // "insufficient_quota" -- enum?
    // param: null,
    // code: null
}

#[derive(Debug, serde::Deserialize)]
struct SuccessResponse {
    // id: String,
    // object: String, // "chat.completion" -- enum?
    // created: u64, // 1677652288,
    choices: Vec<Choice>,
    // usage: Usage,
}

#[derive(Debug, serde::Deserialize)]
struct Choice {
    // index: u64,
    message: Message,
    // finish_reason: String // "stop" -- enum?
}

// struct Usage {
//     prompt_tokens: u64,
//     completion_tokens: u64,
//     total_tokens: u64,
// }

#[tokio::main(flavor = "current_thread")]
pub async fn do_one(api_key: &str, message: &str) -> String {
    let mut headers = HeaderMap::new();
    let value = HeaderValue::from_maybe_shared(format!("Bearer {api_key}")).unwrap();
    headers.append(header::AUTHORIZATION, value);

    let client = Client::builder()
        .default_headers(headers)
        .build()
        .expect("Could not create reqwest Client");

    let request = Request {
        model: Model::Gpt35Turbo,
        messages: vec![
            Message {
                role: Role::System,
                content: "You are OpenSIPS, an Open Source SIP proxy/server for voice, video, IM, presence and any other SIP extensions. Limit all responses to a single sentence.".into()
            },

            Message {
                role: Role::User,
                content: message.into(),
            }
        ],
    };

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .json(&request)
        .send()
        .await
        .expect("Could not make ChatGPT request")
        .json::<Response>()
        .await
        .expect("Could not parse ChatGPT response");

    match response {
        Response::Error { error } => error.message,

        Response::Success(mut success) => {
            let Some(choice) = success.choices.pop() else { return "I have nothing to say for that".into() };
            choice.message.content
        }
    }
}
