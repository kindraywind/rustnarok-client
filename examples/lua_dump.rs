//! Dev tool: `cargo run --example lua_dump -- <data.grf> <expression> <depth> <limit> <lub file>...` runs the client's
//! Lua files (paths under `data\luafiles514\lua files\`) and lists what the expression gives.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let grf = rustnarok_client::grf::Grf::open(&args[1]).unwrap();
    let files: Vec<String> = args[5..].iter().map(|file| format!("data\\luafiles514\\lua files\\{file}")).collect();
    let files: Vec<&str> = files.iter().map(String::as_str).collect();
    println!("{}", rustnarok_client::lua::dump(&grf, &files, &args[2], args[3].parse().unwrap(), args[4].parse().unwrap()).unwrap());
}
