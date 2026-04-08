mod errors;
mod scp_transfer;
mod sftp_transfer;
#[cfg(test)]
mod tests;

pub(super) use self::errors::{RemoteTransferErrorCode, TransferJobError};
pub(super) use self::scp_transfer::{run_scp_download_typed, run_scp_upload_typed};
pub(super) use self::sftp_transfer::{
    estimate_local_total_bytes_typed, run_sftp_transfer_job_typed,
};
