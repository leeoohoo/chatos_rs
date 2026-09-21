#include "ChatOSProcessRuntime.h"

#include <errno.h>
#include <libproc.h>
#include <signal.h>
#include <spawn.h>
#include <sys/wait.h>
#include <unistd.h>

int chatos_spawn_process_group(
    const char *executable,
    char *const argv[],
    char *const envp[],
    const char *working_directory,
    int standard_input,
    int standard_output,
    int standard_error,
    pid_t *spawned_pid
) {
    if (executable == NULL || argv == NULL || envp == NULL || spawned_pid == NULL) {
        return EINVAL;
    }

    posix_spawn_file_actions_t actions;
    posix_spawnattr_t attributes;
    int result = posix_spawn_file_actions_init(&actions);
    if (result != 0) return result;
    result = posix_spawnattr_init(&attributes);
    if (result != 0) {
        posix_spawn_file_actions_destroy(&actions);
        return result;
    }

    result = posix_spawn_file_actions_adddup2(&actions, standard_input, STDIN_FILENO);
    if (result == 0) result = posix_spawn_file_actions_adddup2(&actions, standard_output, STDOUT_FILENO);
    if (result == 0) result = posix_spawn_file_actions_adddup2(&actions, standard_error, STDERR_FILENO);
    if (result == 0 && working_directory != NULL) {
        result = posix_spawn_file_actions_addchdir_np(&actions, working_directory);
    }

    short flags = POSIX_SPAWN_SETPGROUP | POSIX_SPAWN_CLOEXEC_DEFAULT;
    if (result == 0) result = posix_spawnattr_setflags(&attributes, flags);
    // A pgroup value of zero asks posix_spawn to use the child's pid.
    if (result == 0) result = posix_spawnattr_setpgroup(&attributes, 0);
    if (result == 0) {
        result = posix_spawn(spawned_pid, executable, &actions, &attributes, argv, envp);
    }

    posix_spawnattr_destroy(&attributes);
    posix_spawn_file_actions_destroy(&actions);
    return result;
}

int chatos_signal_process_group(pid_t group_id, int signal_number) {
    if (group_id <= 0) return EINVAL;
    if (kill(-group_id, signal_number) == 0 || errno == ESRCH) return 0;
    return errno;
}

int chatos_reap_process(pid_t pid, int *exit_code) {
    if (pid <= 0 || exit_code == NULL) return EINVAL;
    int status = 0;
    pid_t result;
    do {
        result = waitpid(pid, &status, 0);
    } while (result == -1 && errno == EINTR);
    if (result == -1) return errno;
    if (WIFEXITED(status)) {
        *exit_code = WEXITSTATUS(status);
    } else if (WIFSIGNALED(status)) {
        *exit_code = 128 + WTERMSIG(status);
    } else {
        *exit_code = 1;
    }
    return 0;
}

int chatos_try_reap_process(pid_t pid, int *exit_code, int *did_exit) {
    if (pid <= 0 || exit_code == NULL || did_exit == NULL) return EINVAL;
    *did_exit = 0;
    int status = 0;
    pid_t result;
    do {
        result = waitpid(pid, &status, WNOHANG);
    } while (result == -1 && errno == EINTR);
    if (result == 0) return 0;
    if (result == -1) {
        if (errno == ECHILD) {
            *did_exit = 1;
            return 0;
        }
        return errno;
    }
    *did_exit = 1;
    if (WIFEXITED(status)) {
        *exit_code = WEXITSTATUS(status);
    } else if (WIFSIGNALED(status)) {
        *exit_code = 128 + WTERMSIG(status);
    } else {
        *exit_code = 1;
    }
    return 0;
}

int chatos_process_start_time(
    pid_t pid,
    uint64_t *seconds,
    uint64_t *microseconds
) {
    if (pid <= 0 || seconds == NULL || microseconds == NULL) return EINVAL;
    struct proc_bsdinfo info;
    int bytes = proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, sizeof(info));
    if (bytes != sizeof(info)) return errno == 0 ? ESRCH : errno;
    *seconds = (uint64_t)info.pbi_start_tvsec;
    *microseconds = (uint64_t)info.pbi_start_tvusec;
    return 0;
}

int chatos_process_matches_start_time(
    pid_t pid,
    uint64_t seconds,
    uint64_t microseconds
) {
    uint64_t current_seconds = 0;
    uint64_t current_microseconds = 0;
    int result = chatos_process_start_time(
        pid,
        &current_seconds,
        &current_microseconds
    );
    if (result == ESRCH) return 0;
    if (result != 0) return result;
    return current_seconds == seconds && current_microseconds == microseconds ? 1 : 0;
}
