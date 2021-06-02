use notify::{RecursiveMode, immediate_watcher, Watcher, RecommendedWatcher};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::time::{Duration, sleep};
use anyhow::{Context, Result};
use tokio::{spawn, task::JoinHandle};
use std::sync::Arc;

pub struct ProjectWatcher {
    watcher: RecommendedWatcher,
    debouncer: JoinHandle<()>,
}

impl ProjectWatcher {
    pub async fn start(paths: &[String], debounce_time: Duration) -> Result<(Self, UnboundedReceiver<()>)>  {
        let (sender, mut receiver) = mpsc::unbounded_channel();
    
        let mut watcher = immediate_watcher(move |event| {
            if let Err(error) = event {
                return eprintln!("Watcher failed: {:#?}", error);
            }
            if let Err(error) = sender.send(()) {
                return eprintln!("Failed to send with the sender: {:#?}", error);
            }
        }).context("Failed to create the watcher")?;
    
        let configure_context = "Failed to configure the watcher";
        watcher.configure(notify::Config::PreciseEvents(false)).context(configure_context)?;
        watcher.configure(notify::Config::NoticeEvents(false)).context(configure_context)?;
        watcher.configure(notify::Config::OngoingEvents(None)).context(configure_context)?;
    
        for path in paths {
            watcher.watch(path, RecursiveMode::Recursive).context("Failed to set a watched path")?;
        }
    
        let (debounced_sender, debounced_receiver) = mpsc::unbounded_channel();
    
        let debouncer = spawn(async move {
            let mut debounced_task = None::<JoinHandle<()>>;
            let debounced_sender = Arc::new(debounced_sender);
            while receiver.recv().await.is_some() {
                if let Some(debounced_task) = debounced_task {
                    debounced_task.abort();
                }
                let debounced_sender = Arc::clone(&debounced_sender);
                debounced_task = Some(spawn(async move {
                    sleep(debounce_time).await; 
                    if let Err(error) = debounced_sender.send(()) {
                        return eprintln!("Failed to send with the debounced sender: {:#?}", error);
                    }
                }));
            }
            if let Some(debounced_task) = debounced_task {
                debounced_task.abort();
            }
        });

        let this = ProjectWatcher {
            watcher,
            debouncer,
        };
        Ok((this, debounced_receiver))
    }

    pub async fn stop(self) -> Result<()> {
        let watcher = self.watcher;
        drop(watcher);
        self.debouncer.await?;
        Ok(())
    }
}
