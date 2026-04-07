use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;

use super::ffmpeg::{MediaProbe, ProcessSpec, probe_media};
use super::log::Logger;

#[derive(Debug, Clone)]
pub struct AppContext {
    pub dry_run: bool,
    pub jobs: usize,
    pub ffmpeg_bin: String,
    pub ffprobe_bin: String,
    pub logger: Logger,
}

impl AppContext {
    pub fn new(
        dry_run: bool,
        jobs: usize,
        ffmpeg_bin: String,
        ffprobe_bin: String,
        logger: Logger,
    ) -> Result<Self> {
        if jobs == 0 {
            bail!("jobs must be at least 1");
        }

        Ok(Self {
            dry_run,
            jobs,
            ffmpeg_bin,
            ffprobe_bin,
            logger,
        })
    }

    pub fn ffmpeg(&self, args: Vec<String>) -> ProcessSpec {
        ProcessSpec::new(self.ffmpeg_bin.clone(), args)
    }

    pub fn execute_plan(&self, plan: &[ProcessSpec]) -> Result<()> {
        for step in plan {
            self.execute_step(step)?;
        }
        Ok(())
    }

    pub fn execute_step(&self, step: &ProcessSpec) -> Result<()> {
        step.run(&self.logger, self.dry_run)
    }

    pub fn probe_media(&self, input: &Path) -> Result<MediaProbe> {
        if self.dry_run {
            self.logger.debug(format!(
                "probing {} during dry-run because planning depends on metadata",
                input.display()
            ));
        }

        probe_media(&self.ffprobe_bin, input)
    }
}

pub fn run_for_inputs<F>(context: &AppContext, inputs: &[std::path::PathBuf], job: F) -> Result<()>
where
    F: Fn(&Path) -> Result<()> + Send + Sync,
{
    let failures = Arc::new(Mutex::new(Vec::new()));
    let pool = ThreadPoolBuilder::new()
        .num_threads(context.jobs)
        .build()
        .context("failed to build worker pool")?;

    pool.install(|| {
        inputs.par_iter().for_each(|input| {
            context
                .logger
                .info(format!("processing {}", input.display()));
            if let Err(error) = job(input) {
                context
                    .logger
                    .warn(format!("{} failed: {error:#}", input.display()));
                failures
                    .lock()
                    .expect("mutex poisoned")
                    .push(format!("{}: {error:#}", input.display()));
            } else {
                context
                    .logger
                    .success(format!("finished {}", input.display()));
            }
        });
    });

    let failures = failures.lock().expect("mutex poisoned");
    if failures.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(failures.join("\n")))
    }
}
