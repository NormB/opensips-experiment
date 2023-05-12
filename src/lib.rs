use opensips::{cstr_lit, StrExt};
use std::{
    cell::UnsafeCell,
    ffi::CStr,
    fs::Permissions,
    os::raw::{c_char, c_int, c_void},
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

mod chatgpt;

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

static CMDS: &[opensips::cmd_export_t] = &[
    opensips::cmd_export_t {
        name: cstr_lit!("rust_experiment_reply"),
        function: Some(reply),
        params: [opensips::cmd_param::NULL; 9],
        flags: opensips::REQUEST_ROUTE,
    },
    opensips::cmd_export_t {
        name: cstr_lit!("rust_experiment_test_str"),
        function: Some(test_str),
        params: {
            let mut params = [opensips::cmd_param::NULL; 9];
            params[0] = opensips::cmd_param {
                flags: opensips::CMD_PARAM_STR,
                fixup: None,
                free_fixup: None,
            };
            params[1] = opensips::cmd_param {
                flags: opensips::CMD_PARAM_STR,
                fixup: None,
                free_fixup: None,
            };
            params
        },
        flags: opensips::REQUEST_ROUTE,
    },
    opensips::cmd_export_t::NULL,
];

static PARAMS: &[opensips::param_export_t] = &[
    opensips::param_export_t {
        name: cstr_lit!("count"),
        type_: opensips::INT_PARAM,
        param_pointer: COUNT.as_mut().cast(),
    },
    opensips::param_export_t {
        name: cstr_lit!("name"),
        type_: opensips::STR_PARAM,
        param_pointer: NAME.as_mut().cast(),
    },
    opensips::param_export_t {
        name: cstr_lit!("chatgpt-key"),
        type_: opensips::STR_PARAM,
        param_pointer: CHATGPT_KEY.as_mut().cast(),
    },
    opensips::param_export_t::NULL,
];

static COUNT: GlobalIntParam = GlobalIntParam::new();
static NAME: GlobalStrParam = GlobalStrParam::new();
static CHATGPT_KEY: GlobalStrParam = GlobalStrParam::new();

#[repr(C)]
struct GlobalIntParam(UnsafeCell<c_int>);

// This *requires* that the plugin is only used in a single-threaded
// fashion.
unsafe impl Sync for GlobalIntParam {}

impl GlobalIntParam {
    const fn new() -> Self {
        Self(UnsafeCell::new(0))
    }

    fn get(&self) -> c_int {
        unsafe { *self.0.get() }
    }

    const fn as_mut(&self) -> *mut c_int {
        self.0.get()
    }
}

#[repr(C)]
struct GlobalStrParam(UnsafeCell<*mut c_char>);

// This *requires* that the plugin is only used in a single-threaded
// fashion.
unsafe impl Sync for GlobalStrParam {}

impl GlobalStrParam {
    const fn new() -> Self {
        Self(UnsafeCell::new(ptr::null_mut()))
    }

    fn get(&self) -> *mut c_char {
        unsafe { *self.0.get() }
    }

    const fn as_mut(&self) -> *mut *mut c_char {
        self.0.get()
    }
}

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
    chatgpt_key: String,
}

static STATE: RwLock<Option<GlobalState>> = RwLock::new(None);

unsafe extern "C" fn init() -> c_int {
    eprintln!("rust_experiment::init called (PID {})", std::process::id());

    let count = COUNT.get();
    let count = count.try_into().unwrap_or(0);

    let name = NAME.get();
    let name = CStr::from_ptr(name);
    let name = name.to_string_lossy().into();

    let chatgpt_key = CHATGPT_KEY.get();
    let chatgpt_key = CStr::from_ptr(chatgpt_key);
    let chatgpt_key = chatgpt_key.to_string_lossy().into();

    let mut sigb = std::mem::zeroed();
    opensips::load_sig_api(&mut sigb);

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

unsafe extern "C" fn init_child(rank: c_int) -> c_int {
    eprintln!(
        "rust_experiment::init_child called (PID {}, rank {rank})",
        std::process::id()
    );

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
async fn run_server_loop() {
    eprintln!(
        "rust_experiment::run_server_loop called (PID {})",
        std::process::id()
    );

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
                        eprintln!("rust_experiment::run_server_loop worker connected (PID {})", std::process::id());

                        let tx = tx.clone();
                        let b_rx = b_rx.resubscribe();

                        // TODO: track the spawned task
                        tokio::spawn(run_server_child(stream, tx, b_rx));
                    }
                    Err(err) => {
                        eprintln!("{err}");
                        eprintln!("{err:?}");
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

async fn run_server_child(
    stream: UnixStream,
    tx: mpsc::Sender<Message>,
    mut b_rx: broadcast::Receiver<Message>,
) {
    eprintln!(
        "rust_experiment::run_server_child called (PID {})",
        std::process::id()
    );

    let mut stream = BufReader::new(stream);
    let mut data = String::with_capacity(1024);

    loop {
        data.clear();

        select! {
            Ok(msg) = b_rx.recv() => {
                eprintln!("[{}] Got data from broadcast, sending to worker", std::process::id());
                let msg = serde_json::to_vec(&msg).unwrap();
                stream.write_all(&msg).await.unwrap();
                stream.write_all(&[b'\n']).await.unwrap();
                stream.flush().await.unwrap();
            }

            Ok(n_bytes) = stream.read_line(&mut data) => {
                if n_bytes == 0 { break }

                eprintln!("[{}] Got data from worker, broadcasting...", std::process::id());
                let msg = serde_json::from_str(&data).expect("Data was not valid JSON");

                tx.send(msg).await.unwrap();
            }
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn run_worker_loop(mut rx: mpsc::Receiver<Message>) {
    eprintln!(
        "rust_experiment::run_worker_loop called (PID {})",
        std::process::id()
    );

    let stream = UnixStream::connect(CONTROL_SOCKET).await.unwrap();
    let mut stream = BufReader::new(stream);

    let mut data = String::with_capacity(1024);

    loop {
        data.clear();

        select! {
            Some(msg) = rx.recv() => {
                eprintln!("[{}] Got data from channel, sending to parent...", std::process::id());
                let msg = serde_json::to_vec(&msg).unwrap();
                stream.write_all(&msg).await.unwrap();
                stream.write_all(&[b'\n']).await.unwrap();
                stream.flush().await.unwrap();
            }

            Ok(n_bytes) = stream.read_line(&mut data) => {
                if n_bytes == 0 { break }

                eprintln!("[{}], Received data from parent...", std::process::id());

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

unsafe extern "C" fn reply(
    msg: *mut opensips::sip_msg,
    _ctx: *mut c_void,
    _arg2: *mut c_void,
    _arg3: *mut c_void,
    _arg4: *mut c_void,
    _arg5: *mut c_void,
    _arg6: *mut c_void,
    _arg7: *mut c_void,
    _arg8: *mut c_void,
) -> i32 {
    eprintln!("rust_experiment::reply called (PID {})", std::process::id());

    let state = STATE.read().expect("Lock poisoned");
    let state = state.as_ref().expect("Not initialized");
    let msg = &mut *msg;

    let chatgpt_query = msg
        .header_iter()
        .map(|h| (h.name.as_str(), h.body.as_str()))
        .find(|(n, _b)| n.eq_ignore_ascii_case("X-ChatGPT"))
        .map(|(_h, b)| b);

    let chatgpt_response = chatgpt_query.map(|query| chatgpt::do_one(&state.chatgpt_key, query));

    let mut add_header = |name, value| {
        let mut header = String::from(name);
        header.push_str(": ");
        header.push_str(value);
        header.push_str("\n");

        let lump = opensips::add_lump_rpl(
            msg,
            header.as_mut_ptr(),
            header.len().try_into().unwrap(),
            opensips::LUMP_RPL_HDR | opensips::LUMP_RPL_NOFREE,
        );

        !lump.is_null()
    };

    let rust_header_value = format!(
        "{} / {} / {} / {}",
        state.name, state.count, state.counter, state.dog_url
    );
    if !add_header("X-Rust", &rust_header_value) {
        // TODO: error
        return -1;
    }

    if let Some(chatgpt_header_value) = chatgpt_response {
        if !add_header("X-ChatGPT", &chatgpt_header_value) {
            // TODO: error
            return -1;
        }
    }

    let reply = state.sigb.reply.expect("reply function pointer missing");

    let code = 200;
    let reason = &"OK".as_opensips_str();
    let tag = ptr::null_mut();

    if reply(msg, code, reason, tag) == -1 {
        // TODO: LM_ERR("failed to send 200 via send_reply\n");
        return -1;
    }

    0
}

unsafe extern "C" fn test_str(
    _msg: *mut opensips::sip_msg,
    s1: *mut c_void,
    s2: *mut c_void,
    _arg3: *mut c_void,
    _arg4: *mut c_void,
    _arg5: *mut c_void,
    _arg6: *mut c_void,
    _arg7: *mut c_void,
    _arg8: *mut c_void,
) -> i32 {
    eprintln!(
        "rust_experiment::test_str called (PID {})",
        std::process::id()
    );

    let s1 = s1.cast::<opensips::str_>();
    let s2 = s2.cast::<opensips::str_>();

    let s1 = &*s1;
    let s2 = &*s2;

    let s1 = s1.as_str();
    let s2 = s2.as_str();

    s1.contains(s2) as _
}

unsafe extern "C" fn control(
    _params: *const opensips::mi_params_t,
    _async_hdl: *mut opensips::mi_handler,
) -> *mut opensips::mi_response_t {
    eprintln!(
        "rust_experiment::control called (PID {})",
        std::process::id()
    );

    let state = STATE.read().expect("Lock poisoned");
    let state = state.as_ref().expect("Not initialized");
    let parent_tx = state.parent_tx.as_ref().expect("Can't talk to the network");
    parent_tx.blocking_send(Message::IncrementCounter).unwrap();

    opensips::init_mi_result_ok()
}
