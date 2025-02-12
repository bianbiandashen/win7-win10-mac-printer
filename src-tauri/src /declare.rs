#[derive(Clone)]
pub struct PrintOptions {
    pub id: String,
    pub path: String,
    pub printer: String,
    pub taskid: String,
    pub print_setting: String,
    pub remove_after_print: bool,
    pub auto_fit: bool,

}
