use serde::{Deserialize, Serialize};

/// Configuration for a single process/task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessConfig {
    pub name: String,
    /// Period in ms (how often the task is released).
    pub period_ms: u64,
    /// CPU time required per period in ms.
    pub cpu_time_ms: u64,
    /// Static priority (0 = highest). Used as tiebreaker when deadlines are equal.
    #[serde(default)]
    pub priority: u32,
    /// If Some(core), this process is pinned to that core (0-indexed).
    #[serde(default)]
    pub pinned_core: Option<usize>,
}

/// Configuration for the entire simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Scheduler tick period in ms (scheduling granularity).
    pub tick_period_ms: u64,
    /// Total simulation duration in ms.
    pub simulation_duration_ms: u64,
    /// Number of CPU cores (default 1).
    #[serde(default = "default_num_cores")]
    pub num_cores: usize,
    /// If true, once a process starts on a core it stays there (no migration).
    #[serde(default)]
    pub fixed_partitioning: bool,
    /// List of processes to schedule.
    pub processes: Vec<ProcessConfig>,
}

fn default_num_cores() -> usize { 1 }

/// A single entry in the resulting schedule (one contiguous execution block).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleEntry {
    /// Start time in ms.
    pub time_ms: u64,
    /// Duration of this execution block in ms.
    pub duration_ms: u64,
    /// Process name, or "IDLE" if no process was running.
    pub process_name: String,
    /// Which core this ran on (0-indexed).
    pub core: usize,
}

/// A deadline miss event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadlineMiss {
    pub process_name: String,
    /// The time at which the deadline was missed.
    pub deadline_ms: u64,
    /// Remaining CPU time that was not completed.
    pub remaining_ms: u64,
}

/// Result of an EDF simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub schedule: Vec<ScheduleEntry>,
    pub total_duration_ms: u64,
    pub cpu_utilization: f64,
    pub num_cores: usize,
    pub deadline_misses: Vec<DeadlineMiss>,
}

/// Runtime state for a process job.
#[derive(Debug, Clone)]
struct Job {
    process_index: usize,
    deadline_ms: u64,
    remaining_ms: u64,
    priority: u32,
    pinned_core: Option<usize>,
}

impl Job {
    fn sort_key(&self) -> (u64, u32, usize) {
        (self.deadline_ms, self.priority, self.process_index)
    }
}

/// Try to merge a schedule entry with the last entry for the same core,
/// or push a new one.
fn push_or_merge(schedule: &mut Vec<ScheduleEntry>, core: usize, time: u64, dur: u64, name: &str) {
    if let Some(last) = schedule.iter_mut().rev()
        .find(|e| e.core == core)
    {
        if last.process_name == name && last.time_ms + last.duration_ms == time {
            last.duration_ms += dur;
            return;
        }
    }
    schedule.push(ScheduleEntry {
        time_ms: time,
        duration_ms: dur,
        process_name: name.to_string(),
        core,
    });
}

/// Run the EDF scheduling simulation.
pub fn simulate(config: &SchedulerConfig) -> SimulationResult {
    let tick = config.tick_period_ms;
    let duration = config.simulation_duration_ms;
    let processes = &config.processes;
    let num_cores = config.num_cores.max(1);
    let fixed_part = config.fixed_partitioning;

    if tick == 0 || duration == 0 || processes.is_empty() {
        return SimulationResult {
            schedule: vec![],
            total_duration_ms: duration,
            cpu_utilization: 0.0,
            num_cores,
            deadline_misses: vec![],
        };
    }

    let utilization: f64 = processes.iter()
        .map(|p| p.cpu_time_ms as f64 / p.period_ms as f64)
        .sum();

    let mut schedule: Vec<ScheduleEntry> = Vec::new();
    let mut deadline_misses: Vec<DeadlineMiss> = Vec::new();
    let mut ready_jobs: Vec<Job> = Vec::new();
    let mut next_release: Vec<u64> = vec![0; processes.len()];

    // Fixed partitioning: remember which core each process was first assigned to.
    // None means not yet assigned.
    let mut process_affinity: Vec<Option<usize>> = vec![None; processes.len()];

    let mut current_time: u64 = 0;

    while current_time < duration {
        // Release new jobs
        for (i, proc) in processes.iter().enumerate() {
            while next_release[i] <= current_time {
                // Effective pinned_core: explicit pin > fixed partition affinity > None
                let effective_pin = proc.pinned_core.or_else(|| {
                    if fixed_part { process_affinity[i] } else { None }
                });
                ready_jobs.push(Job {
                    process_index: i,
                    deadline_ms: next_release[i] + proc.period_ms,
                    remaining_ms: proc.cpu_time_ms,
                    priority: proc.priority,
                    pinned_core: effective_pin,
                });
                next_release[i] += proc.period_ms;
            }
        }

        // Remove jobs whose deadlines have passed
        let mut surviving = Vec::new();
        for job in ready_jobs.drain(..) {
            if job.deadline_ms <= current_time && job.remaining_ms > 0 {
                deadline_misses.push(DeadlineMiss {
                    process_name: processes[job.process_index].name.clone(),
                    deadline_ms: job.deadline_ms,
                    remaining_ms: job.remaining_ms,
                });
            } else {
                surviving.push(job);
            }
        }
        ready_jobs = surviving;

        // Sort ready jobs by EDF priority (earliest deadline first)
        ready_jobs.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

        // Assign jobs to cores
        let mut core_assignment: Vec<Option<usize>> = vec![None; num_cores]; // index into ready_jobs
        let mut assigned: Vec<bool> = vec![false; ready_jobs.len()];

        // Pass 1: pinned jobs — includes explicit pins AND fixed-partition affinities
        for (ji, job) in ready_jobs.iter().enumerate() {
            if let Some(core) = job.pinned_core {
                if core < num_cores && core_assignment[core].is_none() {
                    core_assignment[core] = Some(ji);
                    assigned[ji] = true;
                }
            }
        }

        // Pass 2: unpinned jobs fill remaining cores (load-balanced: first available core)
        for (ji, job) in ready_jobs.iter().enumerate() {
            if assigned[ji] || job.pinned_core.is_some() {
                continue;
            }
            // Find first free core
            if let Some(core) = core_assignment.iter().position(|a| a.is_none()) {
                core_assignment[core] = Some(ji);
                assigned[ji] = true;
            }
        }

        // Execute one tick on each core
        let exec_time = tick.min(duration - current_time);

        for (core, assignment) in core_assignment.iter().enumerate() {
            if let Some(ji) = assignment {
                let job = &mut ready_jobs[*ji];
                let run = exec_time.min(job.remaining_ms);
                let name = &processes[job.process_index].name;
                push_or_merge(&mut schedule, core, current_time, run, name);
                job.remaining_ms -= run;

                // Fixed partitioning: record affinity on first execution
                // and pin the current job so it won't migrate mid-period
                if fixed_part && process_affinity[job.process_index].is_none() {
                    process_affinity[job.process_index] = Some(core);
                    job.pinned_core = Some(core);
                }
            } else {
                push_or_merge(&mut schedule, core, current_time, exec_time, "IDLE");
            }
        }

        // Remove completed jobs
        ready_jobs.retain(|j| j.remaining_ms > 0);

        current_time += exec_time;
    }

    // End-of-simulation: only count as miss if deadline <= duration
    for job in &ready_jobs {
        if job.remaining_ms > 0 && job.deadline_ms <= duration {
            deadline_misses.push(DeadlineMiss {
                process_name: processes[job.process_index].name.clone(),
                deadline_ms: job.deadline_ms,
                remaining_ms: job.remaining_ms,
            });
        }
    }

    SimulationResult {
        schedule,
        total_duration_ms: duration,
        cpu_utilization: utilization,
        num_cores,
        deadline_misses,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_edf_example() {
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 60,
            num_cores: 1,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 2, priority: 0, pinned_core: None },
                ProcessConfig { name: "B".into(), period_ms: 30, cpu_time_ms: 10, priority: 0, pinned_core: None },
                ProcessConfig { name: "C".into(), period_ms: 60, cpu_time_ms: 20, priority: 0, pinned_core: None },
            ],
        };
        let result = simulate(&config);

        assert!(result.deadline_misses.is_empty(),
            "No deadline misses expected, got: {:?}", result.deadline_misses);

        let total_cpu: u64 = result.schedule.iter()
            .filter(|e| e.process_name != "IDLE")
            .map(|e| e.duration_ms)
            .sum();
        assert_eq!(total_cpu, 52);
        assert!((result.cpu_utilization - 0.8667).abs() < 0.01);
    }

    #[test]
    fn test_single_process() {
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 30,
            num_cores: 1,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 5, priority: 0, pinned_core: None },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty());

        let total_a: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "A")
            .map(|e| e.duration_ms)
            .sum();
        assert_eq!(total_a, 15);
    }

    #[test]
    fn test_overloaded_system() {
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 20,
            num_cores: 1,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 7, priority: 0, pinned_core: None },
                ProcessConfig { name: "B".into(), period_ms: 10, cpu_time_ms: 5, priority: 0, pinned_core: None },
            ],
        };
        let result = simulate(&config);
        assert!(result.cpu_utilization > 1.0);
        assert!(!result.deadline_misses.is_empty());
    }

    #[test]
    fn test_empty_config() {
        let config = SchedulerConfig {
            tick_period_ms: 10,
            simulation_duration_ms: 100,
            num_cores: 1,
            fixed_partitioning: false,
            processes: vec![],
        };
        let result = simulate(&config);
        assert!(result.schedule.is_empty());
    }

    #[test]
    fn test_preemption() {
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 20,
            num_cores: 1,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 2, priority: 0, pinned_core: None },
                ProcessConfig { name: "B".into(), period_ms: 20, cpu_time_ms: 8, priority: 0, pinned_core: None },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty());
        let first = &result.schedule[0];
        assert_eq!(first.process_name, "A");
        assert_eq!(first.time_ms, 0);
        assert_eq!(first.core, 0);
    }

    #[test]
    fn test_multicore_parallel() {
        // Two processes that would overload 1 core but fit on 2
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 10,
            num_cores: 2,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 8, priority: 0, pinned_core: None },
                ProcessConfig { name: "B".into(), period_ms: 10, cpu_time_ms: 8, priority: 0, pinned_core: None },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty(),
            "2 cores should handle 160% load, got misses: {:?}", result.deadline_misses);

        // Both should run in parallel
        let a_on_0: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "A" && e.core == 0)
            .map(|e| e.duration_ms).sum();
        let b_on_1: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "B" && e.core == 1)
            .map(|e| e.duration_ms).sum();
        assert_eq!(a_on_0, 8);
        assert_eq!(b_on_1, 8);
    }

    #[test]
    fn test_core_pinning() {
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 10,
            num_cores: 2,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 5, priority: 0, pinned_core: Some(1) },
                ProcessConfig { name: "B".into(), period_ms: 10, cpu_time_ms: 5, priority: 0, pinned_core: None },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty());

        // A must run on core 1 only
        let a_on_core1: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "A" && e.core == 1)
            .map(|e| e.duration_ms).sum();
        let a_on_core0: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "A" && e.core == 0)
            .map(|e| e.duration_ms).sum();
        assert_eq!(a_on_core1, 5);
        assert_eq!(a_on_core0, 0);

        // B (unpinned) should go to core 0 (first available)
        let b_on_core0: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "B" && e.core == 0)
            .map(|e| e.duration_ms).sum();
        assert_eq!(b_on_core0, 5);
    }

    #[test]
    fn test_fixed_partitioning() {
        // With fixed partitioning, A should stay on its initially assigned core
        // across multiple periods
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 20,
            num_cores: 2,
            fixed_partitioning: true,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 3, priority: 0, pinned_core: None },
                ProcessConfig { name: "B".into(), period_ms: 10, cpu_time_ms: 3, priority: 0, pinned_core: None },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty());

        // A should always run on the same core (core 0, first assigned)
        let a_cores: Vec<usize> = result.schedule.iter()
            .filter(|e| e.process_name == "A")
            .map(|e| e.core)
            .collect();
        assert!(a_cores.iter().all(|&c| c == a_cores[0]),
            "A should stay on one core with fixed partitioning, got cores: {:?}", a_cores);

        // B should always run on the same core (core 1)
        let b_cores: Vec<usize> = result.schedule.iter()
            .filter(|e| e.process_name == "B")
            .map(|e| e.core)
            .collect();
        assert!(b_cores.iter().all(|&c| c == b_cores[0]),
            "B should stay on one core with fixed partitioning, got cores: {:?}", b_cores);

        // They should be on different cores
        assert_ne!(a_cores[0], b_cores[0],
            "A and B should be on different cores");
    }
}
