// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::c_void;

use anyhow::{anyhow, Result};

use super::{
    AXUIElementCopyAttributeValue, AXUIElementCreateSystemWide, AXUIElementGetPid,
    AXUIElementGetTypeID, AXUIElementIsAttributeSettable, AXUIElementSetMessagingTimeout,
    AXValueGetType, AXValueGetTypeID, AXValueGetValue, CFBooleanGetTypeID, CFBooleanGetValue,
    CFEqual, CFGetTypeID, CFRelease, CFRetain, CFStringCreateWithBytes, CFStringGetTypeID, CGPoint,
    CGSize,
};

const AX_ERROR_SUCCESS: i32 = 0;
const AX_ERROR_ATTRIBUTE_UNSUPPORTED: i32 = -25205;
const AX_ERROR_NO_VALUE: i32 = -25212;
const AX_VALUE_TYPE_CGPOINT: u32 = 1;
const AX_VALUE_TYPE_CGSIZE: u32 = 2;
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

#[derive(Debug)]
struct MacCfObject(*const c_void);

impl MacCfObject {
    fn from_owned(value: *const c_void, label: &str) -> Result<Self> {
        if value.is_null() {
            return Err(anyhow!("macOS Accessibility returned no {label}"));
        }
        Ok(Self(value))
    }

    fn string(value: &str) -> Result<Self> {
        let byte_count = isize::try_from(value.len())
            .map_err(|_| anyhow!("macOS Accessibility attribute name is too long"))?;
        // SAFETY: CoreFoundation copies the bounded UTF-8 bytes into one retained CFString.
        let string = unsafe {
            CFStringCreateWithBytes(
                std::ptr::null(),
                value.as_bytes().as_ptr(),
                byte_count,
                CF_STRING_ENCODING_UTF8,
                0,
            )
        };
        Self::from_owned(string, "CoreFoundation string")
    }

    fn as_ptr(&self) -> *const c_void {
        self.0
    }
}

impl Clone for MacCfObject {
    fn clone(&self) -> Self {
        // SAFETY: self owns a live CoreFoundation reference, and the clone balances this retain in
        // its Drop implementation.
        unsafe {
            CFRetain(self.0);
        }
        Self(self.0)
    }
}

impl Drop for MacCfObject {
    fn drop(&mut self) {
        // SAFETY: every object is created from a retained CoreFoundation result or CFRetain and is
        // released exactly once here.
        unsafe {
            CFRelease(self.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MacTextTargetClass {
    NativeTextControl,
    ContentEditable,
}

impl MacTextTargetClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::NativeTextControl => "native_text_control",
            Self::ContentEditable => "contenteditable",
        }
    }
}

pub(super) fn classify_macos_text_target(
    native_role: bool,
    rich_text_role: bool,
    explicitly_editable: bool,
    value_settable: bool,
    selection_range_settable: bool,
) -> Result<MacTextTargetClass> {
    if native_role {
        if value_settable || selection_range_settable {
            return Ok(MacTextTargetClass::NativeTextControl);
        }
        return Err(anyhow!(
            "Computer Use refuses to type into a read-only macOS text control"
        ));
    }
    if rich_text_role && explicitly_editable && selection_range_settable {
        return Ok(MacTextTargetClass::ContentEditable);
    }
    Err(anyhow!(
        "Computer Use text input requires a focused writable native text control or explicit contenteditable target"
    ))
}

#[derive(Debug)]
pub(super) struct ValidatedMacTextTarget {
    application: MacCfObject,
    focused: MacCfObject,
    target: MacCfObject,
    pid: i32,
    class: MacTextTargetClass,
}

impl ValidatedMacTextTarget {
    pub(super) fn validate() -> Result<Self> {
        // SAFETY: AXUIElementCreateSystemWide returns one retained process-local accessibility
        // object. The wrapper releases it after all bounded synchronous queries complete.
        let system = MacCfObject::from_owned(
            unsafe { AXUIElementCreateSystemWide() },
            "system-wide Accessibility element",
        )?;
        // SAFETY: this limits AX messaging only inside the one-shot helper process. No input has
        // been posted at this point, and failure aborts before target inspection.
        let timeout_status = unsafe { AXUIElementSetMessagingTimeout(system.as_ptr(), 2.0) };
        if timeout_status != AX_ERROR_SUCCESS {
            return Err(anyhow!(
                "macOS Accessibility messaging timeout setup failed: {timeout_status}"
            ));
        }
        let application = required_ax_element(&system, "AXFocusedApplication")?;
        if !required_ax_bool(&application, "AXFrontmost")? {
            return Err(anyhow!(
                "Computer Use text input requires the focused macOS application to remain frontmost"
            ));
        }
        let pid = ax_element_pid(&application)?;
        if pid <= 0 {
            return Err(anyhow!(
                "macOS Accessibility returned an invalid foreground process identity"
            ));
        }
        let focused = required_ax_element(&application, "AXFocusedUIElement")?;
        if ax_element_pid(&focused)? != pid {
            return Err(anyhow!(
                "macOS focused text element does not belong to the frontmost application"
            ));
        }
        if !required_ax_bool(&focused, "AXEnabled")? || !required_ax_bool(&focused, "AXFocused")? {
            return Err(anyhow!(
                "Computer Use text input requires an enabled, keyboard-focused macOS control"
            ));
        }
        ensure_macos_text_element_is_not_secure(&focused)?;

        let focused_native = ax_string_matches(
            &focused,
            "AXRole",
            &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"],
        )?;
        let focused_editable = optional_ax_bool(&focused, "AXIsEditable")? == Some(true);
        let target = if focused_native || focused_editable {
            focused.clone()
        } else {
            optional_ax_element(&focused, "AXEditableAncestor")?
                .or(optional_ax_element(&focused, "AXHighestEditableAncestor")?)
                .ok_or_else(|| {
                    anyhow!("Computer Use text input requires an explicit macOS editable target")
                })?
        };
        if ax_element_pid(&target)? != pid {
            return Err(anyhow!(
                "macOS editable target does not belong to the frontmost application"
            ));
        }
        if !required_ax_bool(&target, "AXEnabled")? {
            return Err(anyhow!(
                "Computer Use text input requires an enabled macOS editable target"
            ));
        }
        ensure_macos_text_element_is_not_secure(&target)?;
        ensure_nonempty_ax_bounds(&target)?;

        let native_role = ax_string_matches(
            &target,
            "AXRole",
            &["AXTextField", "AXTextArea", "AXComboBox", "AXSearchField"],
        )?;
        let rich_text_role =
            ax_string_matches(&target, "AXRole", &["AXWebArea", "AXGroup", "AXStaticText"])?;
        let explicitly_editable = optional_ax_bool(&target, "AXIsEditable")? == Some(true);
        let value_settable = optional_ax_attribute_settable(&target, "AXValue")? == Some(true);
        let selection_range_settable =
            optional_ax_attribute_settable(&target, "AXSelectedTextRange")? == Some(true);
        let class = classify_macos_text_target(
            native_role,
            rich_text_role,
            explicitly_editable,
            value_settable,
            selection_range_settable,
        )?;
        Ok(Self {
            application,
            focused,
            target,
            pid,
            class,
        })
    }

    pub(super) fn ensure_still_focused(&self) -> Result<()> {
        let current = Self::validate()?;
        if current.pid != self.pid
            || current.class != self.class
            || !cf_equal(&current.application, &self.application)
            || !cf_equal(&current.focused, &self.focused)
            || !cf_equal(&current.target, &self.target)
        {
            return Err(anyhow!(
                "macOS focused editable target changed after validation"
            ));
        }
        Ok(())
    }

    pub(super) fn class_name(&self) -> &'static str {
        self.class.as_str()
    }
}

fn ax_copy_attribute(element: &MacCfObject, attribute: &str) -> Result<Option<MacCfObject>> {
    let attribute_name = MacCfObject::string(attribute)?;
    let mut value = std::ptr::null();
    // SAFETY: element and attribute_name are live CoreFoundation objects. On success the API
    // writes one retained value, which is immediately transferred into wrapper ownership.
    let status = unsafe {
        AXUIElementCopyAttributeValue(element.as_ptr(), attribute_name.as_ptr(), &mut value)
    };
    match status {
        AX_ERROR_SUCCESS => MacCfObject::from_owned(value, attribute).map(Some),
        AX_ERROR_ATTRIBUTE_UNSUPPORTED | AX_ERROR_NO_VALUE => Ok(None),
        _ => Err(anyhow!(
            "macOS Accessibility could not read {attribute}: {status}"
        )),
    }
}

fn required_ax_element(element: &MacCfObject, attribute: &str) -> Result<MacCfObject> {
    optional_ax_element(element, attribute)?
        .ok_or_else(|| anyhow!("macOS Accessibility did not provide required {attribute} identity"))
}

fn optional_ax_element(element: &MacCfObject, attribute: &str) -> Result<Option<MacCfObject>> {
    let Some(value) = ax_copy_attribute(element, attribute)? else {
        return Ok(None);
    };
    // SAFETY: value owns a live CoreFoundation object for the duration of this type query.
    let value_type = unsafe { CFGetTypeID(value.as_ptr()) };
    // SAFETY: AXUIElementGetTypeID returns a process-stable numeric type identifier.
    if value_type != unsafe { AXUIElementGetTypeID() } {
        return Err(anyhow!(
            "macOS Accessibility returned an invalid {attribute} element type"
        ));
    }
    Ok(Some(value))
}

fn required_ax_bool(element: &MacCfObject, attribute: &str) -> Result<bool> {
    optional_ax_bool(element, attribute)?
        .ok_or_else(|| anyhow!("macOS Accessibility did not provide required {attribute} state"))
}

fn optional_ax_bool(element: &MacCfObject, attribute: &str) -> Result<Option<bool>> {
    let Some(value) = ax_copy_attribute(element, attribute)? else {
        return Ok(None);
    };
    // SAFETY: value owns a live CoreFoundation object for both type and boolean queries.
    if unsafe { CFGetTypeID(value.as_ptr()) } != unsafe { CFBooleanGetTypeID() } {
        return Err(anyhow!(
            "macOS Accessibility returned an invalid {attribute} boolean type"
        ));
    }
    // SAFETY: the type identity above proves value is a CFBoolean.
    Ok(Some(unsafe { CFBooleanGetValue(value.as_ptr()) } != 0))
}

fn optional_ax_attribute_settable(element: &MacCfObject, attribute: &str) -> Result<Option<bool>> {
    let attribute_name = MacCfObject::string(attribute)?;
    let mut settable = 0_u8;
    // SAFETY: element and attribute_name are live objects and the API writes one Boolean.
    let status = unsafe {
        AXUIElementIsAttributeSettable(element.as_ptr(), attribute_name.as_ptr(), &mut settable)
    };
    match status {
        AX_ERROR_SUCCESS => Ok(Some(settable != 0)),
        AX_ERROR_ATTRIBUTE_UNSUPPORTED | AX_ERROR_NO_VALUE => Ok(None),
        _ => Err(anyhow!(
            "macOS Accessibility could not confirm whether {attribute} is writable: {status}"
        )),
    }
}

fn ax_string_matches(element: &MacCfObject, attribute: &str, expected: &[&str]) -> Result<bool> {
    let Some(value) = ax_copy_attribute(element, attribute)? else {
        return Ok(false);
    };
    // SAFETY: value owns a live CoreFoundation object for this type query.
    if unsafe { CFGetTypeID(value.as_ptr()) } != unsafe { CFStringGetTypeID() } {
        return Err(anyhow!(
            "macOS Accessibility returned an invalid {attribute} string type"
        ));
    }
    for candidate in expected {
        let candidate = MacCfObject::string(candidate)?;
        if cf_equal(&value, &candidate) {
            return Ok(true);
        }
    }
    Ok(false)
}

fn ax_element_pid(element: &MacCfObject) -> Result<i32> {
    let mut pid = 0_i32;
    // SAFETY: element is a validated AXUIElement and pid points to one writable process id.
    let status = unsafe { AXUIElementGetPid(element.as_ptr(), &mut pid) };
    if status != AX_ERROR_SUCCESS {
        return Err(anyhow!(
            "macOS Accessibility could not read process identity: {status}"
        ));
    }
    Ok(pid)
}

fn ensure_macos_text_element_is_not_secure(element: &MacCfObject) -> Result<()> {
    let secure_role = ax_string_matches(
        element,
        "AXRole",
        &[
            "AXSecureTextField",
            "AXPasswordField",
            "AXPasswordTextField",
        ],
    )? || ax_string_matches(
        element,
        "AXSubrole",
        &[
            "AXSecureTextField",
            "AXPasswordField",
            "AXPasswordTextField",
        ],
    )?;
    let protected = optional_ax_bool(element, "AXContainsProtectedContent")? == Some(true);
    if secure_role || protected {
        return Err(anyhow!(
            "Computer Use refuses to type into a secure, password, or protected macOS field"
        ));
    }
    Ok(())
}

fn ensure_nonempty_ax_bounds(element: &MacCfObject) -> Result<()> {
    let position = required_ax_value(element, "AXPosition", AX_VALUE_TYPE_CGPOINT)?;
    let size = required_ax_value(element, "AXSize", AX_VALUE_TYPE_CGSIZE)?;
    let mut point = CGPoint { x: 0.0, y: 0.0 };
    let mut dimensions = CGSize {
        width: 0.0,
        height: 0.0,
    };
    // SAFETY: both retained values were type-checked for the exact destination structures.
    let point_ok = unsafe {
        AXValueGetValue(
            position.as_ptr(),
            AX_VALUE_TYPE_CGPOINT,
            (&mut point as *mut CGPoint).cast(),
        )
    } != 0;
    let size_ok = unsafe {
        AXValueGetValue(
            size.as_ptr(),
            AX_VALUE_TYPE_CGSIZE,
            (&mut dimensions as *mut CGSize).cast(),
        )
    } != 0;
    if !point_ok
        || !size_ok
        || !point.x.is_finite()
        || !point.y.is_finite()
        || !dimensions.width.is_finite()
        || !dimensions.height.is_finite()
        || dimensions.width <= 0.0
        || dimensions.height <= 0.0
    {
        return Err(anyhow!(
            "Computer Use text input requires a visible macOS editable target with non-empty bounds"
        ));
    }
    Ok(())
}

fn required_ax_value(
    element: &MacCfObject,
    attribute: &str,
    expected_value_type: u32,
) -> Result<MacCfObject> {
    let value = ax_copy_attribute(element, attribute)?.ok_or_else(|| {
        anyhow!("macOS Accessibility did not provide required {attribute} bounds")
    })?;
    // SAFETY: value owns a live CoreFoundation object for both type queries.
    if unsafe { CFGetTypeID(value.as_ptr()) } != unsafe { AXValueGetTypeID() }
        || unsafe { AXValueGetType(value.as_ptr()) } != expected_value_type
    {
        return Err(anyhow!(
            "macOS Accessibility returned invalid {attribute} bounds"
        ));
    }
    Ok(value)
}

fn cf_equal(left: &MacCfObject, right: &MacCfObject) -> bool {
    // SAFETY: both arguments own live CoreFoundation objects for the duration of the comparison.
    unsafe { CFEqual(left.as_ptr(), right.as_ptr()) != 0 }
}
