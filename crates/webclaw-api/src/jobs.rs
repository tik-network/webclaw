use std::time::Instant;

use dashmap::DashMap;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    Running,
    Completed,
    Failed,
}

pub struct CrawlJob {
    pub id: String,
    pub status: JobStatus,
    pub url: String,
    pub result: Option<webclaw_fetch::CrawlResult>,
    pub error: Option<String>,
    pub created_at: Instant,
}

pub struct JobStore {
    jobs: DashMap<String, CrawlJob>,
}

impl JobStore {
    pub fn new() -> Self {
        Self {
            jobs: DashMap::new(),
        }
    }

    pub fn insert(&self, job: CrawlJob) {
        self.jobs.insert(job.id.clone(), job);
    }

    pub fn get(
        &self,
        id: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, CrawlJob>> {
        self.jobs.get(id)
    }

    pub fn update_completed(&self, id: &str, result: webclaw_fetch::CrawlResult) {
        if let Some(mut job) = self.jobs.get_mut(id) {
            job.status = JobStatus::Completed;
            job.result = Some(result);
        }
    }

    pub fn update_failed(&self, id: &str, error: String) {
        if let Some(mut job) = self.jobs.get_mut(id) {
            job.status = JobStatus::Failed;
            job.error = Some(error);
        }
    }
}
