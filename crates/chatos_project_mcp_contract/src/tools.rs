pub const GET_PROJECT_OVERVIEW: &str = "get_project_overview";
pub const INITIALIZE_PROJECT: &str = "initialize_project";
pub const LIST_REQUIREMENTS: &str = "list_requirements";
pub const CREATE_REQUIREMENT: &str = "create_requirement";
pub const UPDATE_REQUIREMENT: &str = "update_requirement";
pub const DELETE_REQUIREMENT: &str = "delete_requirement";
pub const SET_REQUIREMENT_DEPENDENCIES: &str = "set_requirement_dependencies";
pub const UPSERT_REQUIREMENT_TECHNICAL_OVERVIEW: &str = "upsert_requirement_technical_overview";
pub const GET_REQUIREMENT_TECHNICAL_OVERVIEW: &str = "get_requirement_technical_overview";
pub const LIST_PROJECT_TASKS: &str = "list_project_tasks";
pub const CREATE_PROJECT_TASK: &str = "create_project_task";
pub const UPDATE_PROJECT_TASK: &str = "update_project_task";
pub const DELETE_PROJECT_TASK: &str = "delete_project_task";
pub const SET_PROJECT_TASK_DEPENDENCIES: &str = "set_project_task_dependencies";
pub const GET_PROJECT_DEPENDENCY_GRAPH: &str = "get_project_dependency_graph";

pub const PROJECT_MANAGEMENT_SERVER_TOOL_NAMES: &[&str] = &[
    GET_PROJECT_OVERVIEW,
    INITIALIZE_PROJECT,
    LIST_REQUIREMENTS,
    CREATE_REQUIREMENT,
    UPDATE_REQUIREMENT,
    DELETE_REQUIREMENT,
    SET_REQUIREMENT_DEPENDENCIES,
    UPSERT_REQUIREMENT_TECHNICAL_OVERVIEW,
    GET_REQUIREMENT_TECHNICAL_OVERVIEW,
    LIST_PROJECT_TASKS,
    CREATE_PROJECT_TASK,
    UPDATE_PROJECT_TASK,
    DELETE_PROJECT_TASK,
    SET_PROJECT_TASK_DEPENDENCIES,
    GET_PROJECT_DEPENDENCY_GRAPH,
];

pub const TASK_RUNNER_BUILTIN_TOOL_NAMES: &[&str] = &[
    GET_PROJECT_OVERVIEW,
    INITIALIZE_PROJECT,
    LIST_REQUIREMENTS,
    CREATE_REQUIREMENT,
    UPDATE_REQUIREMENT,
    SET_REQUIREMENT_DEPENDENCIES,
    UPSERT_REQUIREMENT_TECHNICAL_OVERVIEW,
    GET_REQUIREMENT_TECHNICAL_OVERVIEW,
    LIST_PROJECT_TASKS,
    CREATE_PROJECT_TASK,
    UPDATE_PROJECT_TASK,
    SET_PROJECT_TASK_DEPENDENCIES,
    GET_PROJECT_DEPENDENCY_GRAPH,
];

pub fn owned_names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}
