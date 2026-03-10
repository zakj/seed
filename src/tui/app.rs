use crate::error::Error;
use crate::store::Store;
use crate::task::Task;

#[allow(dead_code)] // Fields used by upcoming TUI tasks
pub struct App {
    pub store: Store,
    pub tasks: Vec<Task>,
    pub selected: usize,
}

impl App {
    pub fn new(store: Store) -> Result<Self, Error> {
        let tasks = store.load_all_tasks()?;
        Ok(Self {
            store,
            tasks,
            selected: 0,
        })
    }
}
