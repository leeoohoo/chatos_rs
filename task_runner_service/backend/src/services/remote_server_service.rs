use crate::auth::CurrentUser;
use crate::models::{
    CreateRemoteServerRequest, RemoteServerRecord, RemoteServerTestResponse,
    TestRemoteServerRequest, UpdateRemoteServerRequest, now_rfc3339,
};
use crate::remote_server_runtime::test_remote_server_connectivity;
use crate::store::AppStore;

use super::remote_servers::{
    build_remote_server_record, normalize_remote_server_auth_type,
    normalize_remote_server_host_key_policy, normalize_remote_server_port,
    validate_remote_server_auth_fields,
};
use super::{RemoteServerService, normalized_optional, validate_required};

mod crud;
mod testing;
