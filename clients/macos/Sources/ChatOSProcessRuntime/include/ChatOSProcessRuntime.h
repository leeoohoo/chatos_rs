#ifndef CHATOS_PROCESS_RUNTIME_H
#define CHATOS_PROCESS_RUNTIME_H

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

#endif
