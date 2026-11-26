// `sqlx::migrate!` embeds the migrations at compile time but does not make
// cargo watch the directory on stable: a new migration file would not
// trigger a rebuild.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
