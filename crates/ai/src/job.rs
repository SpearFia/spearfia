// SPDX-License-Identifier: MIT OR Apache-2.0
//! Windows kills the whole CLI process tree even if the coordinator crashes.
#[cfg(windows)]
pub struct ProcessTree(std::os::windows::io::OwnedHandle);
#[cfg(not(windows))]
pub struct ProcessTree;

impl ProcessTree {
    pub fn attach(child: &std::process::Child) -> std::io::Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
            use windows_sys::Win32::System::JobObjects::*;
            // SAFETY: null security/name creates a new unnamed, non-inheritable job.
            let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if raw.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: CreateJobObjectW transferred ownership of this valid handle.
            let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
            // SAFETY: all-zero values initialize the limits structure; only the documented flag is set.
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: live job handle and correctly-sized limits buffer; neither pointer escapes the call.
            if unsafe {
                SetInformationJobObject(
                    raw,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    std::mem::size_of_val(&limits) as u32,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: both handles are live for the duration of this call.
            if unsafe { AssignProcessToJobObject(raw, child.as_raw_handle()) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Self(handle))
        }
        #[cfg(not(windows))]
        {
            let _ = child;
            Ok(Self)
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        // OwnedHandle closes after this drop; referencing it keeps ownership explicit.
        let _ = &self.0;
    }
}
