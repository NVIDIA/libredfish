use crate::model::{
    task::{Task, TaskState},
    ODataLinks,
};
use serde::{Deserialize, Serialize};

// A "Job" is very similar to a "Task", but there are a few key differences.
// We won't export this struct for now, instead cramming the info into a "Task" struct for ease of use.

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub id: Option<String>,
    pub name: Option<String>,
    pub percent_complete: Option<u32>,
    pub job_state: Option<TaskState>,
}

impl Job {
    pub fn as_task(&self) -> Task {
        Task {
            odata: self.odata.clone(),
            id: self.id.clone().unwrap_or("".to_string()),
            messages: vec![],
            name: self.name.clone(),
            task_state: self.job_state.clone(),
            task_status: None,
            task_monitor: None,
            percent_complete: self.percent_complete,
        }
    }
}
