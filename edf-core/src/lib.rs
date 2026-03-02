use serde::{Deserialize, Serialize};

/// Configuration for a single process/task.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    /// List of process names that must complete before this process can start.
    /// When set, this process's jobs will not be scheduled until all dependencies
    /// have finished their current-period execution.
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// List of dependency names that use double buffering.
    /// Double-buffered edges allow the consumer to start immediately using
    /// data from the previous period (latency = 1 period).
    #[serde(default)]
    pub double_buffer_deps: Vec<String>,
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

/// Per-process response time metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessMetrics {
    pub name: String,
    pub period_ms: u64,
    pub cpu_time_ms: u64,
    pub num_jobs: u64,
    pub num_completions: u64,
    pub best_response_ms: Option<u64>,
    pub worst_response_ms: Option<u64>,
    pub avg_response_ms: f64,
    pub jitter_ms: u64,
    pub best_slack_ms: Option<i64>,
    pub worst_slack_ms: Option<i64>,
}

/// End-to-end chain latency metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainMetrics {
    pub chain: Vec<String>,
    pub best_e2e_ms: Option<u64>,
    pub worst_e2e_ms: Option<u64>,
    pub avg_e2e_ms: f64,
}

/// Result of an EDF simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    pub schedule: Vec<ScheduleEntry>,
    pub total_duration_ms: u64,
    pub cpu_utilization: f64,
    pub num_cores: usize,
    pub deadline_misses: Vec<DeadlineMiss>,
    pub process_metrics: Vec<ProcessMetrics>,
    pub chain_metrics: Vec<ChainMetrics>,
}

/// Runtime state for a process job.
#[derive(Debug, Clone)]
struct Job {
    process_index: usize,
    deadline_ms: u64,
    remaining_ms: u64,
    priority: u32,
    pinned_core: Option<usize>,
    release_time: u64,
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
            process_metrics: vec![],
            chain_metrics: vec![],
        };
    }

    let utilization: f64 = processes.iter()
        .map(|p| p.cpu_time_ms as f64 / p.period_ms as f64)
        .sum();

    // Build dependency map: process_index -> Vec<dependency process indices>
    let dep_map: Vec<Vec<usize>> = processes.iter().map(|p| {
        p.dependencies.iter().filter_map(|dep_name| {
            processes.iter().position(|q| q.name == *dep_name)
        }).collect()
    }).collect();

    // Multi-rate FIFO: compute fifo_depth per dependency edge.
    // fifo_depth[i][j_pos] = how many completions of dep j process i needs before it can run.
    // When consumer period > producer period, depth = consumer_period / producer_period.
    let fifo_depth: Vec<Vec<u64>> = processes.iter().enumerate().map(|(i, p)| {
        dep_map[i].iter().map(|&dep_idx| {
            let period_self = p.period_ms;
            let period_dep = processes[dep_idx].period_ms;
            if period_self > period_dep && period_dep > 0 {
                period_self / period_dep
            } else {
                1
            }
        }).collect()
    }).collect();

    // Build double-buffer map: is_double_buffer[i][j_pos] = true if edge is double-buffered
    let is_double_buffer: Vec<Vec<bool>> = processes.iter().enumerate().map(|(i, p)| {
        dep_map[i].iter().map(|&dep_idx| {
            let dep_name = &processes[dep_idx].name;
            p.double_buffer_deps.contains(dep_name)
        }).collect()
    }).collect();

    // FIFO counters: tracks how many completions of each dependency have accumulated
    // since the consumer last consumed them.
    // Double-buffered edges start pre-filled to depth (consumer can run immediately).
    let mut dep_fifo_counters: Vec<Vec<i64>> =
        fifo_depth.iter().enumerate().map(|(i, row)| {
            row.iter().enumerate().map(|(j, &depth)| {
                if is_double_buffer[i][j] { depth as i64 } else { 0i64 }
            }).collect()
        }).collect();

    let mut schedule: Vec<ScheduleEntry> = Vec::new();
    let mut deadline_misses: Vec<DeadlineMiss> = Vec::new();
    let mut ready_jobs: Vec<Job> = Vec::new();
    let mut next_release: Vec<u64> = vec![0; processes.len()];

    // Fixed partitioning: remember which core each process was first assigned to.
    let mut process_affinity: Vec<Option<usize>> = vec![None; processes.len()];

    // Metrics collection: (release_time, completion_time, deadline) per process
    let mut job_records: Vec<Vec<(u64, u64, u64)>> = vec![vec![]; processes.len()];
    let mut job_counts: Vec<u64> = vec![0; processes.len()];

    let mut current_time: u64 = 0;

    while current_time < duration {
        // Release new jobs at tick boundary
        for (i, proc) in processes.iter().enumerate() {
            while next_release[i] <= current_time {
                let effective_pin = proc.pinned_core.or_else(|| {
                    if fixed_part { process_affinity[i] } else { None }
                });
                ready_jobs.push(Job {
                    process_index: i,
                    deadline_ms: next_release[i] + proc.period_ms,
                    remaining_ms: proc.cpu_time_ms,
                    priority: proc.priority,
                    pinned_core: effective_pin,
                    release_time: next_release[i],
                });
                job_counts[i] += 1;
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

        // Sub-tick loop: execute within this tick, re-evaluating on every job completion
        let tick_end = (current_time + tick).min(duration);
        let mut sub_time = current_time;

        while sub_time < tick_end {
            // Sort ready jobs by EDF priority
            ready_jobs.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

            // Check dependency blocking (FIFO counters)
            let mut blocked: Vec<bool> = vec![false; ready_jobs.len()];
            for (ji, job) in ready_jobs.iter().enumerate() {
                let i = job.process_index;
                let deps = &dep_map[i];
                if !deps.is_empty() {
                    for (j_pos, _dep_idx) in deps.iter().enumerate() {
                        let depth = fifo_depth[i][j_pos];
                        if dep_fifo_counters[i][j_pos] < depth as i64 {
                            blocked[ji] = true;
                            break;
                        }
                    }
                }
            }

            // Assign jobs to cores
            let mut core_assignment: Vec<Option<usize>> = vec![None; num_cores];
            let mut assigned: Vec<bool> = vec![false; ready_jobs.len()];

            // Pass 1: pinned jobs
            for (ji, job) in ready_jobs.iter().enumerate() {
                if blocked[ji] { continue; }
                if let Some(core) = job.pinned_core {
                    if core < num_cores && core_assignment[core].is_none() {
                        core_assignment[core] = Some(ji);
                        assigned[ji] = true;
                    }
                }
            }

            // Pass 2: unpinned jobs fill remaining cores
            for (ji, job) in ready_jobs.iter().enumerate() {
                if blocked[ji] || assigned[ji] || job.pinned_core.is_some() {
                    continue;
                }
                if let Some(core) = core_assignment.iter().position(|a| a.is_none()) {
                    core_assignment[core] = Some(ji);
                    assigned[ji] = true;
                }
            }

            // Determine how far to advance: min of remaining tick time and
            // earliest job completion among assigned jobs
            let remaining_tick = tick_end - sub_time;
            let mut advance = remaining_tick;
            for assignment in core_assignment.iter() {
                if let Some(ji) = assignment {
                    advance = advance.min(ready_jobs[*ji].remaining_ms);
                }
            }
            // Safety: always advance at least 1ms to avoid infinite loops
            if advance == 0 { advance = remaining_tick; }

            // Execute the sub-interval on each core
            for (core, assignment) in core_assignment.iter().enumerate() {
                if let Some(ji) = assignment {
                    let job = &mut ready_jobs[*ji];
                    let run = advance.min(job.remaining_ms);
                    let name = &processes[job.process_index].name;
                    push_or_merge(&mut schedule, core, sub_time, run, name);
                    job.remaining_ms -= run;

                    if fixed_part && process_affinity[job.process_index].is_none() {
                        process_affinity[job.process_index] = Some(core);
                        job.pinned_core = Some(core);
                    }
                } else {
                    push_or_merge(&mut schedule, core, sub_time, advance, "IDLE");
                }
            }

            sub_time += advance;

            // Detect completed jobs, record metrics, update FIFO counters
            let mut newly_completed: Vec<usize> = Vec::new();
            let mut completed_releases: Vec<(usize, u64, u64)> = Vec::new(); // (idx, release, deadline)
            ready_jobs.retain(|j| {
                if j.remaining_ms == 0 {
                    newly_completed.push(j.process_index);
                    completed_releases.push((j.process_index, j.release_time, j.deadline_ms));
                    false
                } else {
                    true
                }
            });

            // Record completion times for metrics
            for &(proc_idx, release, deadline) in &completed_releases {
                job_records[proc_idx].push((release, sub_time, deadline));
            }

            // Increment FIFO counters for consumers of completed processes
            for &completed_idx in &newly_completed {
                for (i, deps) in dep_map.iter().enumerate() {
                    for (j_pos, &dep_idx) in deps.iter().enumerate() {
                        if dep_idx == completed_idx {
                            dep_fifo_counters[i][j_pos] += 1;
                        }
                    }
                }
            }

            // Consume FIFOs when a consumer completes
            for &completed_idx in &newly_completed {
                let period_self = processes[completed_idx].period_ms;
                for (j_pos, &dep_idx) in dep_map[completed_idx].iter().enumerate() {
                    let period_dep = processes[dep_idx].period_ms;
                    if period_self >= period_dep {
                        let depth = fifo_depth[completed_idx][j_pos] as i64;
                        dep_fifo_counters[completed_idx][j_pos] =
                            (dep_fifo_counters[completed_idx][j_pos] - depth).max(0);
                    }
                }
            }

            // If jobs completed, the inner loop will re-sort and re-assign
            // on the next iteration, filling freed cores immediately
        }

        current_time = tick_end;
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

    // Compute per-process metrics
    let process_metrics: Vec<ProcessMetrics> = processes.iter().enumerate().map(|(i, p)| {
        let records = &job_records[i];
        let num_completions = records.len() as u64;
        let (best_rt, worst_rt, avg_rt, jitter, best_slack, worst_slack) = if records.is_empty() {
            (None, None, 0.0, 0, None, None)
        } else {
            let response_times: Vec<u64> = records.iter()
                .map(|&(release, completion, _)| completion - release).collect();
            let slacks: Vec<i64> = records.iter()
                .map(|&(_, completion, deadline)| deadline as i64 - completion as i64).collect();
            let best = *response_times.iter().min().unwrap();
            let worst = *response_times.iter().max().unwrap();
            let avg = response_times.iter().sum::<u64>() as f64 / response_times.len() as f64;
            let bs = *slacks.iter().max().unwrap(); // best slack = largest margin
            let ws = *slacks.iter().min().unwrap(); // worst slack = smallest margin
            (Some(best), Some(worst), avg, worst - best, Some(bs), Some(ws))
        };
        ProcessMetrics {
            name: p.name.clone(),
            period_ms: p.period_ms,
            cpu_time_ms: p.cpu_time_ms,
            num_jobs: job_counts[i],
            num_completions,
            best_response_ms: best_rt,
            worst_response_ms: worst_rt,
            avg_response_ms: avg_rt,
            jitter_ms: jitter,
            best_slack_ms: best_slack,
            worst_slack_ms: worst_slack,
        }
    }).collect();

    // Compute chain metrics: for each dependency chain (A→B→...→Z),
    // measure end-to-end latency from A's release to Z's completion
    let chain_metrics = compute_chain_metrics(processes, &dep_map, &job_records);

    SimulationResult {
        schedule,
        total_duration_ms: duration,
        cpu_utilization: utilization,
        num_cores,
        deadline_misses,
        process_metrics,
        chain_metrics,
    }
}

/// Find all maximal dependency chains and compute end-to-end latency.
/// A chain is a path from a root (no incoming deps) to a leaf (no outgoing deps).
fn compute_chain_metrics(
    processes: &[ProcessConfig],
    dep_map: &[Vec<usize>],
    job_records: &[Vec<(u64, u64, u64)>],
) -> Vec<ChainMetrics> {
    let n = processes.len();

    // Build forward adjacency (producer → consumers)
    let mut forward: Vec<Vec<usize>> = vec![vec![]; n];
    for (consumer, deps) in dep_map.iter().enumerate() {
        for &producer in deps {
            forward[producer].push(consumer);
        }
    }

    // Find roots (processes with no dependencies)
    let roots: Vec<usize> = (0..n).filter(|&i| dep_map[i].is_empty()).collect();

    // DFS to enumerate all maximal chains
    let mut chains: Vec<Vec<usize>> = Vec::new();
    let mut stack: Vec<(usize, Vec<usize>)> = roots.iter().map(|&r| (r, vec![r])).collect();
    while let Some((node, path)) = stack.pop() {
        if forward[node].is_empty() && path.len() > 1 {
            // Leaf of a chain with at least 2 nodes
            chains.push(path);
        } else if forward[node].is_empty() {
            // Single node, skip
        } else {
            for &next in &forward[node] {
                let mut new_path = path.clone();
                new_path.push(next);
                stack.push((next, new_path));
            }
        }
    }

    // For each chain, compute E2E latency by matching producer completions
    // to consumer completions within the same "reaction" cycle
    chains.iter().map(|chain| {
        let chain_names: Vec<String> = chain.iter().map(|&i| processes[i].name.clone()).collect();
        let first = chain[0];
        let last = chain[chain.len() - 1];

        // Simple approach: for each completion of the last process,
        // find the most recent release of the first process that could have
        // contributed to this output (release_time <= completion of last)
        let first_records = &job_records[first];
        let last_records = &job_records[last];

        if first_records.is_empty() || last_records.is_empty() {
            return ChainMetrics {
                chain: chain_names,
                best_e2e_ms: None,
                worst_e2e_ms: None,
                avg_e2e_ms: 0.0,
            };
        }

        // Match: for each last completion, find the latest first-release before the last-release
        let mut e2e_latencies: Vec<u64> = Vec::new();
        for &(last_release, last_completion, _) in last_records {
            // Find the first process release that triggered this chain execution.
            // Match by release time: the latest first-process release <= last-process release.
            if let Some(&(first_release, _, _)) = first_records.iter()
                .filter(|&&(fr, _, _)| fr <= last_release)
                .last()
            {
                let e2e = last_completion - first_release;
                e2e_latencies.push(e2e);
            }
        }

        if e2e_latencies.is_empty() {
            return ChainMetrics {
                chain: chain_names,
                best_e2e_ms: None,
                worst_e2e_ms: None,
                avg_e2e_ms: 0.0,
            };
        }

        let best = *e2e_latencies.iter().min().unwrap();
        let worst = *e2e_latencies.iter().max().unwrap();
        let avg = e2e_latencies.iter().sum::<u64>() as f64 / e2e_latencies.len() as f64;

        ChainMetrics {
            chain: chain_names,
            best_e2e_ms: Some(best),
            worst_e2e_ms: Some(worst),
            avg_e2e_ms: avg,
        }
    }).collect()
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
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 2, ..Default::default() },
                ProcessConfig { name: "B".into(), period_ms: 30, cpu_time_ms: 10, ..Default::default() },
                ProcessConfig { name: "C".into(), period_ms: 60, cpu_time_ms: 20, ..Default::default() },
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
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 5, ..Default::default() },
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
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 7, ..Default::default() },
                ProcessConfig { name: "B".into(), period_ms: 10, cpu_time_ms: 5, ..Default::default() },
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
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 2, ..Default::default() },
                ProcessConfig { name: "B".into(), period_ms: 20, cpu_time_ms: 8, ..Default::default() },
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
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 8, ..Default::default() },
                ProcessConfig { name: "B".into(), period_ms: 10, cpu_time_ms: 8, ..Default::default() },
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
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 5, pinned_core: Some(1), ..Default::default() },
                ProcessConfig { name: "B".into(), period_ms: 10, cpu_time_ms: 5, ..Default::default() },
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
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 3, ..Default::default() },
                ProcessConfig { name: "B".into(), period_ms: 10, cpu_time_ms: 3, ..Default::default() },
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

    #[test]
    fn test_dependency_chain() {
        // B depends on A: B should not start until A finishes.
        // On 2 cores without dependencies, they would run in parallel.
        // With dependencies, B must wait for A to complete.
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 20,
            num_cores: 2,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 20, cpu_time_ms: 5, ..Default::default() },
                ProcessConfig { name: "B".into(), period_ms: 20, cpu_time_ms: 5, priority: 1, dependencies: vec!["A".into()], ..Default::default() },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty(),
            "No deadline misses expected, got: {:?}", result.deadline_misses);

        // Find the earliest start time of B
        let b_start = result.schedule.iter()
            .filter(|e| e.process_name == "B")
            .map(|e| e.time_ms)
            .min()
            .expect("B should have run");

        // Find the end time of A (first job)
        let a_end = result.schedule.iter()
            .filter(|e| e.process_name == "A")
            .map(|e| e.time_ms + e.duration_ms)
            .max()
            .expect("A should have run");

        assert!(b_start >= a_end,
            "B should start at {} (after A ends at {}), but started at {}",
            a_end, a_end, b_start);
    }

    #[test]
    fn test_multirate_fifo() {
        // A(10ms, 2ms CPU) → B(30ms, 5ms CPU)
        // B must wait for 3 completions of A before it can run (FIFO depth = 3).
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 90,
            num_cores: 1,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 2, ..Default::default() },
                ProcessConfig { name: "B".into(), period_ms: 30, cpu_time_ms: 5, priority: 1, dependencies: vec!["A".into()], ..Default::default() },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty(),
            "No deadline misses expected, got: {:?}", result.deadline_misses);

        // B should not start before 3 completions of A.
        // A completes at t=2, t=12, t=22 (first 3 completions).
        // So B's first possible start is t=22.
        let b_first_start = result.schedule.iter()
            .filter(|e| e.process_name == "B")
            .map(|e| e.time_ms)
            .min()
            .expect("B should have run");
        assert!(b_first_start >= 22,
            "B should start at or after t=22 (after 3 A completions), but started at {}", b_first_start);

        // B should have run 3 times over 90ms (period=30ms)
        let b_total_cpu: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "B")
            .map(|e| e.duration_ms)
            .sum();
        assert_eq!(b_total_cpu, 15, "B should run 3 times × 5ms = 15ms total");
    }

    #[test]
    fn test_multirate_chain_fast_consumer() {
        // Gain/1(10ms, 2ms) → NR(30ms, 5ms) → Gain/2(10ms, 2ms)
        // - NR needs 3 completions of Gain/1 (FIFO depth 3)
        // - Gain/2 uses sample-and-hold from NR: runs at 10ms rate,
        //   waits for NR's first completion, then runs freely.
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 90,
            num_cores: 1,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "Gain/1".into(), period_ms: 10, cpu_time_ms: 2, ..Default::default() },
                ProcessConfig { name: "NR".into(), period_ms: 30, cpu_time_ms: 5, priority: 1, dependencies: vec!["Gain/1".into()], ..Default::default() },
                ProcessConfig { name: "Gain/2".into(), period_ms: 10, cpu_time_ms: 2, priority: 2, dependencies: vec!["NR".into()], ..Default::default() },
            ],
        };
        let result = simulate(&config);

        // Gain/1 should run 9 times (90ms / 10ms)
        let g1_total: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "Gain/1")
            .map(|e| e.duration_ms).sum();
        assert_eq!(g1_total, 18, "Gain/1 should run 9 × 2ms = 18ms");

        // NR should run 3 times (90ms / 30ms)
        let nr_total: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "NR")
            .map(|e| e.duration_ms).sum();
        assert_eq!(nr_total, 15, "NR should run 3 × 5ms = 15ms");

        // NR should not start before t=22 (after 3 Gain/1 completions at t=2,12,22)
        let nr_first = result.schedule.iter()
            .filter(|e| e.process_name == "NR")
            .map(|e| e.time_ms).min().expect("NR should have run");
        assert!(nr_first >= 22, "NR should start at or after t=22, started at {}", nr_first);

        // Gain/2 should eventually run after NR's first completion
        let g2_entries: Vec<_> = result.schedule.iter()
            .filter(|e| e.process_name == "Gain/2")
            .collect();
        assert!(!g2_entries.is_empty(), "Gain/2 should have run");

        // Gain/2 should run multiple times after NR's first completion (sample-and-hold)
        let g2_total: u64 = g2_entries.iter().map(|e| e.duration_ms).sum();
        assert!(g2_total > 2,
            "Gain/2 should run more than once (sample-and-hold), total CPU: {}ms", g2_total);

        // Check for deadline misses — with tight scheduling some may occur,
        // but Gain/1 and NR should not miss
        let g1_misses: Vec<_> = result.deadline_misses.iter()
            .filter(|m| m.process_name == "Gain/1").collect();
        let nr_misses: Vec<_> = result.deadline_misses.iter()
            .filter(|m| m.process_name == "NR").collect();
        assert!(g1_misses.is_empty(), "Gain/1 should not miss deadlines: {:?}", g1_misses);
        assert!(nr_misses.is_empty(), "NR should not miss deadlines: {:?}", nr_misses);
    }

    #[test]
    fn test_no_dependency_runs_parallel() {
        // Without dependencies, A and B should run in parallel on 2 cores
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 10,
            num_cores: 2,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 5, ..Default::default() },
                ProcessConfig { name: "B".into(), period_ms: 10, cpu_time_ms: 5, priority: 1, ..Default::default() },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty());

        // Both should start at time 0 (parallel execution)
        let a_start = result.schedule.iter()
            .filter(|e| e.process_name == "A")
            .map(|e| e.time_ms)
            .min().unwrap();
        let b_start = result.schedule.iter()
            .filter(|e| e.process_name == "B")
            .map(|e| e.time_ms)
            .min().unwrap();
        assert_eq!(a_start, 0);
        assert_eq!(b_start, 0, "Without dependencies, B should start at time 0 in parallel");
    }

    #[test]
    fn test_double_buffer_same_rate() {
        // A(10ms) → B(10ms) with double buffering.
        // B should start immediately at t=0 (pre-filled buffer from "previous period").
        // Without double buffer, B would wait for A to complete first.
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 30,
            num_cores: 2,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 3, ..Default::default() },
                ProcessConfig {
                    name: "B".into(), period_ms: 10, cpu_time_ms: 3, priority: 1,
                    dependencies: vec!["A".into()],
                    double_buffer_deps: vec!["A".into()],
                    ..Default::default()
                },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty(),
            "No deadline misses expected, got: {:?}", result.deadline_misses);

        // With double buffer + 2 cores, B should start at t=0 (parallel with A)
        let b_start = result.schedule.iter()
            .filter(|e| e.process_name == "B")
            .map(|e| e.time_ms).min().unwrap();
        assert_eq!(b_start, 0,
            "Double-buffered B should start at t=0, started at {}", b_start);
    }

    #[test]
    fn test_double_buffer_slow_consumer() {
        // A(10ms, 2ms) → B(30ms, 5ms) with double buffering.
        // Without DB: B waits for 3 completions of A (earliest t=22).
        // With DB: B can start immediately (pre-filled with 3 "previous" samples).
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 90,
            num_cores: 2,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 2, ..Default::default() },
                ProcessConfig {
                    name: "B".into(), period_ms: 30, cpu_time_ms: 5, priority: 1,
                    dependencies: vec!["A".into()],
                    double_buffer_deps: vec!["A".into()],
                    ..Default::default()
                },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty(),
            "No deadline misses expected, got: {:?}", result.deadline_misses);

        // With double buffer, B should start at t=0 (not waiting for A)
        let b_start = result.schedule.iter()
            .filter(|e| e.process_name == "B")
            .map(|e| e.time_ms).min().unwrap();
        assert_eq!(b_start, 0,
            "Double-buffered B should start at t=0, started at {}", b_start);

        // B should still run 3 times over 90ms
        let b_total: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "B")
            .map(|e| e.duration_ms).sum();
        assert_eq!(b_total, 15, "B should run 3 × 5ms = 15ms total");
    }

    #[test]
    fn test_double_buffer_fast_consumer() {
        // NR(30ms) → Gain/2(10ms) with double buffering.
        // Without DB: Gain/2 waits for NR's first completion (sample-and-hold).
        // With DB: Gain/2 starts immediately.
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 90,
            num_cores: 2,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "NR".into(), period_ms: 30, cpu_time_ms: 5, ..Default::default() },
                ProcessConfig {
                    name: "Gain/2".into(), period_ms: 10, cpu_time_ms: 2, priority: 1,
                    dependencies: vec!["NR".into()],
                    double_buffer_deps: vec!["NR".into()],
                    ..Default::default()
                },
            ],
        };
        let result = simulate(&config);

        // Gain/2 should start at t=0 (double-buffered, no wait for NR)
        let g2_start = result.schedule.iter()
            .filter(|e| e.process_name == "Gain/2")
            .map(|e| e.time_ms).min().unwrap();
        assert_eq!(g2_start, 0,
            "Double-buffered Gain/2 should start at t=0, started at {}", g2_start);

        // Gain/2 should run 9 times (90ms / 10ms)
        let g2_total: u64 = result.schedule.iter()
            .filter(|e| e.process_name == "Gain/2")
            .map(|e| e.duration_ms).sum();
        assert_eq!(g2_total, 18, "Gain/2 should run 9 × 2ms = 18ms total");
    }

    #[test]
    fn test_intra_tick_rescheduling() {
        // A(20ms, 3ms) → B(20ms, 3ms) on 1 core with tick=5ms.
        // Without intra-tick: A runs [0-3], core idles [3-5], B starts at t=5.
        // With intra-tick: A runs [0-3], B starts immediately at t=3.
        let config = SchedulerConfig {
            tick_period_ms: 5,
            simulation_duration_ms: 20,
            num_cores: 1,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 20, cpu_time_ms: 3, ..Default::default() },
                ProcessConfig {
                    name: "B".into(), period_ms: 20, cpu_time_ms: 3, priority: 1,
                    dependencies: vec!["A".into()],
                    ..Default::default()
                },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty(),
            "No deadline misses expected, got: {:?}", result.deadline_misses);

        // B should start at t=3 (right after A completes), not t=5
        let b_start = result.schedule.iter()
            .filter(|e| e.process_name == "B")
            .map(|e| e.time_ms)
            .min()
            .expect("B should have run");
        assert_eq!(b_start, 3,
            "With intra-tick rescheduling, B should start at t=3, started at {}", b_start);
    }

    #[test]
    fn test_process_metrics() {
        // Simple chain: A(10ms, 2ms) → B(10ms, 3ms) on 1 core, 30ms simulation
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 30,
            num_cores: 1,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 10, cpu_time_ms: 2, ..Default::default() },
                ProcessConfig {
                    name: "B".into(), period_ms: 10, cpu_time_ms: 3, priority: 1,
                    dependencies: vec!["A".into()],
                    ..Default::default()
                },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty());

        // Check process metrics exist
        assert_eq!(result.process_metrics.len(), 2);

        let a_metrics = result.process_metrics.iter().find(|m| m.name == "A").unwrap();
        assert_eq!(a_metrics.num_jobs, 3, "A should have 3 jobs over 30ms");
        assert_eq!(a_metrics.num_completions, 3, "A should complete all 3 jobs");
        assert_eq!(a_metrics.best_response_ms, Some(2)); // A runs first, takes 2ms
        assert!(a_metrics.worst_slack_ms.unwrap() > 0, "A should have positive slack");

        let b_metrics = result.process_metrics.iter().find(|m| m.name == "B").unwrap();
        assert_eq!(b_metrics.num_completions, 3, "B should complete all 3 jobs");
        // B starts after A (at t=2), takes 3ms, so response = 5ms
        assert_eq!(b_metrics.best_response_ms, Some(5));
    }

    #[test]
    fn test_chain_metrics() {
        // A → B → C chain, all 20ms period
        let config = SchedulerConfig {
            tick_period_ms: 1,
            simulation_duration_ms: 40,
            num_cores: 1,
            fixed_partitioning: false,
            processes: vec![
                ProcessConfig { name: "A".into(), period_ms: 20, cpu_time_ms: 2, ..Default::default() },
                ProcessConfig {
                    name: "B".into(), period_ms: 20, cpu_time_ms: 3, priority: 1,
                    dependencies: vec!["A".into()],
                    ..Default::default()
                },
                ProcessConfig {
                    name: "C".into(), period_ms: 20, cpu_time_ms: 2, priority: 2,
                    dependencies: vec!["B".into()],
                    ..Default::default()
                },
            ],
        };
        let result = simulate(&config);
        assert!(result.deadline_misses.is_empty());

        // Should have chain A → B → C
        assert!(!result.chain_metrics.is_empty(), "Should have chain metrics");
        let chain = result.chain_metrics.iter()
            .find(|c| c.chain.len() == 3 && c.chain[0] == "A" && c.chain[2] == "C");
        assert!(chain.is_some(), "Should have A→B→C chain, got: {:?}", result.chain_metrics);
        let chain = chain.unwrap();
        // E2E latency: A starts at 0, C completes at 7 (2+3+2), so e2e = 7ms
        assert!(chain.worst_e2e_ms.unwrap() <= 20,
            "E2E should be well within period, got {}ms", chain.worst_e2e_ms.unwrap());
    }
}
