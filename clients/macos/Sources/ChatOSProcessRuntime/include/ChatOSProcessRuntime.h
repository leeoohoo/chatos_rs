#ifndef CHATOS_PROCESS_RUNTIME_H
#define CHATOS_PROCESS_RUNTIME_H

#include <stdint.h>
#include <sys/types.h>

/// Spawns one process in a new process group whose id is the child's pid.
/// The caller owns all supplied file descriptors and closes the child ends
/// after this function returns.
int chatos_spawn_process_group(
    const char *executable,
    char *const argv[],
    char *const envp[],
    const char *working_directory,
    int standard_input,
    int standard_output,
    int standard_error,
    pid_t *spawned_pid
);

/// Signals the complete plugin process group, including grandchildren.
int chatos_signal_process_group(pid_t group_id, int signal_number);

/// Reaps a child and returns a Process-compatible exit code. Signal exits are
/// reported as 128 + signal.
int chatos_reap_process(pid_t pid, int *exit_code);

/// Non-blocking child reap. did_exit is set to 1 when a child was reaped, or
/// when it was already reaped elsewhere; otherwise it remains 0.
int chatos_try_reap_process(pid_t pid, int *exit_code, int *did_exit);

/// Reads the kernel process start timestamp. The timestamp lets callers
/// distinguish a recorded child from a later process that reused the same pid.
int chatos_process_start_time(
    pid_t pid,
    uint64_t *seconds,
    uint64_t *microseconds
);

/// Returns 1 only when pid still identifies the recorded process, 0 when the
/// process is gone or the pid was reused, and a positive errno value on error.
int chatos_process_matches_start_time(
    pid_t pid,
    uint64_t seconds,
    uint64_t microseconds
);

#endif
