/// A line of the Export textures window's console.
#[derive(Clone, Debug, PartialEq)]
pub enum ConsoleLine {
    Step(String),
    /// "Exporting texture set <name>..." with the name in bold.
    Set(String),
    /// A success line, in green.
    Done(String),
}
