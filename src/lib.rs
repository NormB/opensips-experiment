use opensips::{cstr_lit, module_parameter, StrExt};
use std::{
    fs::Permissions,
    num::NonZeroI32,
    os::raw::{c_char, c_int},
    os::unix::fs::PermissionsExt,
    ptr,
    sync::RwLock,
    thread,
    time::Duration,
};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    select,
    sync::{broadcast, mpsc},
};
use tracing::{error, info, instrument};

mod chatgpt;
mod formatter;

// With a lot of FFI interaction, these safety comments are applicable
// in multiple locations.
//
// SAFETY: [OpenSIPS::valid] It is the responsibility of OpenSIPS and
// how it invokes the modules to ensure that these values are set to
// valid values.

#[no_mangle]
pub static exports: opensips::module_exports = opensips::module_exports {
    name: cstr_lit!("rust-experiment"),
    type_: opensips::module_type::MOD_TYPE_DEFAULT,
    version: opensips::OPENSIPS_FULL_VERSION.as_ptr(),
    compile_flags: opensips::OPENSIPS_COMPILE_FLAGS.as_ptr(),
    dlflags: opensips::DEFAULT_DLFLAGS,
    load_f: None,
    deps: DEPS.as_ptr(),
    cmds: CMDS.as_ptr(),
    acmds: ptr::null(),
    params: PARAMS.as_ptr(),
    stats: ptr::null(),
    mi_cmds: MI_EXPORTS.as_ptr(),
    items: ptr::null(),
    trans: ptr::null(),
    procs: ptr::null(),
    preinit_f: None,
    init_f: Some(init),
    response_f: None,
    destroy_f: None,
    init_child_f: Some(init_child),
    reload_ack_f: None,
};

static DEPS: opensips::dep_export_concrete<1> = opensips::dep_export_concrete {
    md: {
        let mut md = [opensips::module_dependency::NULL; 10];
        md[0] = opensips::module_dependency {
            mod_type: opensips::module_type::MOD_TYPE_DEFAULT,
            mod_name: cstr_lit!(mut "signaling"),
            type_: opensips::DEP_ABORT,
        };
        md
    },
    mpd: [opensips::modparam_dependency::NULL],
};

opensips::commands! {
    #[name = "rust_experiment_reply"]
    fn reply;

    #[name = "rust_experiment_test_str"]
    fn test_str;
}

opensips::module_parameters! {
    #[name = "count"]
    static COUNT: module_parameter::Integer;

    #[name = "name"]
    static NAME: module_parameter::String;

    #[name = "chatgpt-key"]
    static CHATGPT_KEY: module_parameter::String;
}

const DEFAULT_NAME: &str = "This is the default name";

static MI_EXPORTS: &[opensips::mi_export_t] = &[
    opensips::mi_export_t {
        name: cstr_lit!(mut "rust_experiment_control"),
        help: cstr_lit!(mut ""),
        flags: 0,
        init_f: None,
        recipes: {
            let mut recipes = [opensips::mi_recipe_t::NULL; 48];
            recipes[0] = opensips::mi_recipe_t {
                cmd: Some(control),
                params: [ptr::null_mut(); 10],
            };
            recipes
        },
    },
    opensips::mi_export_t::NULL,
];

#[derive(Debug)]
struct GlobalState {
    count: u32,
    name: String,
    counter: u32,
    dog_url: String,
    sigb: opensips::sig_binds,
    parent_tx: Option<mpsc::Sender<Message>>,
    chatgpt_key: Option<String>,
}

static STATE: RwLock<Option<GlobalState>> = RwLock::new(None);

extern "C" fn init() -> c_int {
    formatter::install();

    // `#[instrument]` doesn't work until the formatter is installed.
    let _span = tracing::info_span!("init").entered();

    info!("called");

    let count = COUNT.get_value().map_or(0, NonZeroI32::get);
    let count = count.try_into().unwrap_or(0);

    let name;
    let chatgpt_key;

    // SAFETY: It is the responsibility of OpenSips to set these
    // values to valid C strings.
    unsafe {
        name = NAME.get_value().unwrap_or(DEFAULT_NAME).into();
        chatgpt_key = CHATGPT_KEY.get_value().map(Into::into);
    }

    let Some(sigb) = opensips::load_sig_api() else { return -1 };

    let mut state = STATE.write().expect("Lock poisoned");
    assert!(state.is_none(), "Double-initializing the module");

    *state = Some(GlobalState {
        count,
        name,
        counter: 0,
        dog_url: "Dog URL not set yet".into(),
        sigb,
        parent_tx: None,
        chatgpt_key,
    });

    // TODO: track the spawned thread
    thread::spawn(run_server_loop);

    0
}

#[instrument]
extern "C" fn init_child(rank: c_int) -> c_int {
    info!("called");

    let (tx, rx) = mpsc::channel(3);

    // TODO: track the spawned thread
    thread::spawn(|| run_worker_loop(rx));

    let mut state = STATE.write().expect("Lock poisoned");
    let mut state = state.as_mut().expect("State uninitialized");
    state.parent_tx = Some(tx);

    0
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum Message {
    IncrementCounter,
    NewDog(String),
}

const CONTROL_SOCKET: &str = "/usr/local/etc/opensips/rust_experiment";

#[tokio::main(flavor = "current_thread")]
#[instrument]
async fn run_server_loop() {
    info!("called");

    // We don't care if deleting fails as binding will tell us.
    let _ = fs::remove_file(CONTROL_SOCKET).await;
    let listener = UnixListener::bind(CONTROL_SOCKET).unwrap();
    // TODO: Find minimal appropriate permissions
    fs::set_permissions(CONTROL_SOCKET, Permissions::from_mode(0o777))
        .await
        .unwrap();

    let (tx, mut rx) = mpsc::channel(3);
    let (b_tx, b_rx) = broadcast::channel(3);

    tokio::spawn(run_api_task(tx.clone()));

    loop {
        select! {
            connection = listener.accept() => {
                match connection {
                    Ok((stream, _addr)) => {
                        info!("worker connected");

                        let tx = tx.clone();
                        let b_rx = b_rx.resubscribe();

                        // TODO: track the spawned task
                        tokio::spawn(run_server_child(stream, tx, b_rx));
                    }
                    Err(err) => {
                        error!("{err} / {err:?}");
                        break;
                    }
                }
            }

            Some(msg) = rx.recv() => {
                b_tx.send(msg).unwrap();
            }
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RandomDogResponse {
    // file_size_bytes: u64,
    url: String,
}

async fn run_api_task(tx: mpsc::Sender<Message>) {
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    // Burn the first tick as we want to wait a bit before making the first request
    interval.tick().await;

    loop {
        interval.tick().await;
        let random_dog = reqwest::get("https://random.dog/woof.json")
            .await
            .expect("Could not make HTTP request")
            .json::<RandomDogResponse>()
            .await
            .expect("Could not deserialize API response");

        tx.send(Message::NewDog(random_dog.url)).await.unwrap();
    }
}

#[instrument(skip_all)]
async fn run_server_child(
    stream: UnixStream,
    tx: mpsc::Sender<Message>,
    mut b_rx: broadcast::Receiver<Message>,
) {
    info!("called");

    let mut stream = BufReader::new(stream);
    let mut data = String::with_capacity(1024);

    loop {
        data.clear();

        select! {
            Ok(msg) = b_rx.recv() => {
                info!("Got data from broadcast, sending to worker");
                let msg = serde_json::to_vec(&msg).unwrap();
                stream.write_all(&msg).await.unwrap();
                stream.write_all(&[b'\n']).await.unwrap();
                stream.flush().await.unwrap();
            }

            Ok(n_bytes) = stream.read_line(&mut data) => {
                if n_bytes == 0 { break }

                info!("Got data from worker, broadcasting...");
                let msg = serde_json::from_str(&data).expect("Data was not valid JSON");

                tx.send(msg).await.unwrap();
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
#[instrument(skip_all)]
async fn run_worker_loop(mut rx: mpsc::Receiver<Message>) {
    info!("called");

    let stream = UnixStream::connect(CONTROL_SOCKET).await.unwrap();
    let mut stream = BufReader::new(stream);

    let mut data = String::with_capacity(1024);

    loop {
        data.clear();

        select! {
            Some(msg) = rx.recv() => {
                info!("Got data from channel, sending to parent...");
                let msg = serde_json::to_vec(&msg).unwrap();
                stream.write_all(&msg).await.unwrap();
                stream.write_all(&[b'\n']).await.unwrap();
                stream.flush().await.unwrap();
            }

            Ok(n_bytes) = stream.read_line(&mut data) => {
                if n_bytes == 0 { break }

                info!("Received data from parent...");

                let msg = serde_json::from_str(&data).expect("Data was not valid JSON");
                match msg {
                    Message::IncrementCounter => {
                        let mut state = STATE.write().expect("Lock poisoned");
                        let mut state = state.as_mut().expect("State uninitialized");
                        state.counter = state.counter.wrapping_add(1);
                        // unlock via drop
                    }
                    Message::NewDog(url) => {
                        let mut state = STATE.write().expect("Lock poisoned");
                        let mut state = state.as_mut().expect("State uninitialized");
                        state.dog_url = url;
                    }
                }
            }
        }
    }
}

#[instrument(skip_all)]
fn reply(msg: &mut opensips::sip_msg) -> i32 {
    info!("called");

    let state = STATE.read().expect("Lock poisoned");
    let state = state.as_ref().expect("Not initialized");

    let do_chatgpt = || {
        let key = state.chatgpt_key.as_deref()?;

        let query = msg
            .header_iter()
            .map(|h| (h.name.as_str(), h.body.as_str()))
            .find(|(n, _b)| n.eq_ignore_ascii_case("X-ChatGPT"))
            .map(|(_h, b)| b)?;

        Some(chatgpt::do_one(key, query))
    };

    let chatgpt_response = do_chatgpt();

    let mut add_header = |name, value| {
        let mut header = String::from(name);
        header.push_str(": ");
        header.push_str(value);
        header.push('\n');

        // SAFETY: `msg` is passed from OpenSIPS, the header is
        // managed by Rust's `String`, and we instruct OpenSIPS to not
        // free our memory.
        let lump = unsafe {
            opensips::add_lump_rpl(
                msg,
                header.as_mut_ptr(),
                header.len().try_into().unwrap(),
                opensips::LUMP_RPL_HDR | opensips::LUMP_RPL_NOFREE,
            )
        };

        !lump.is_null()
    };

    let rust_header_value = format!(
        "{} / {} / {} / {}",
        state.name, state.count, state.counter, state.dog_url
    );
    if !add_header("X-Rust", &rust_header_value) {
        error!("Unable to add the X-Rust header");
        return -1;
    }

    if let Some(chatgpt_header_value) = chatgpt_response {
        if !add_header("X-ChatGPT", &chatgpt_header_value) {
            error!("Unable to add the X-ChatGPT header");
            return -1;
        }
    }

    let reply = state.sigb.reply.expect("reply function pointer missing");

    let code = 200;
    let reason = &"OK".as_opensips_str();
    let tag = ptr::null_mut();

    // SAFETY: `msg` comes from OpenSIPS, `code` is an integer,
    // `reason` is a static string, and `tag` is NULL. Nothing bad can
    // happen with those values.
    if unsafe { reply(msg, code, reason, tag) } == -1 {
        error!("failed to reply with 200");
        return -1;
    }

    0
}

#[instrument]
fn test_str(_: &mut opensips::sip_msg, s1: &str, s2: &str) -> i32 {
    info!("called");

    s1.contains(s2) as _
}

#[instrument(skip_all)]
extern "C" fn control(
    _params: *const opensips::mi_params_t,
    _async_hdl: *mut opensips::mi_handler,
) -> *mut opensips::mi_response_t {
    info!("called");
    let state = STATE.read().expect("Lock poisoned");
    let state = state.as_ref().expect("Not initialized");
    let parent_tx = state.parent_tx.as_ref().expect("Can't talk to the network");
    parent_tx.blocking_send(Message::IncrementCounter).unwrap();

    opensips::init_mi_result_ok()
}
