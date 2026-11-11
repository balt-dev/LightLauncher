pub enum Frontend<'l> {
    GUI(&'l mut crate::gui::App),
    CLI,
}
