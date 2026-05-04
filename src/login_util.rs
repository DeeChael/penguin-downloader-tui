use std::time::Duration;

use penguin_downloader::{
    error::Result,
    provider::{
        AccountLoginMethod, CodeLoginCallback, CodeLoginMethod, LoginMethod, LoginMethodType,
        QrLoginCallback, QrLoginMethod, UrlLoginCallback, UrlLoginMethod,
    },
};

pub struct MethodInfo {
    pub id: String,
    pub name: String,
    pub method_type: LoginMethodType,
}

pub fn list_method_info(methods: &[Box<dyn LoginMethod>]) -> Vec<MethodInfo> {
    methods
        .iter()
        .map(|m| MethodInfo {
            id: m.id().to_string(),
            name: m.name().to_string(),
            method_type: m.method_type(),
        })
        .collect()
}

unsafe fn data_ptr_of(method: &dyn LoginMethod) -> *const () {
    let fat: *const dyn LoginMethod = method;
    std::mem::transmute::<*const dyn LoginMethod, (*const (), *const ())>(fat).0
}

pub fn perform_qr_login(
    method: &dyn LoginMethod,
    callback: Box<dyn QrLoginCallback>,
    timeout: Duration,
) -> Result<String> {
    assert_eq!(method.method_type(), LoginMethodType::QR);
    unsafe {
        let qr = &*(data_ptr_of(method) as *const QrLoginMethod);
        qr.start_login(callback, timeout)
    }
}

#[allow(dead_code)]
pub fn perform_url_login(
    method: &dyn LoginMethod,
    callback: Box<dyn UrlLoginCallback>,
    timeout: Duration,
) -> Result<String> {
    assert_eq!(method.method_type(), LoginMethodType::URL);
    unsafe {
        let url = &*(data_ptr_of(method) as *const UrlLoginMethod);
        url.start_login(callback, timeout)
    }
}

pub fn perform_account_login(
    method: &dyn LoginMethod,
    username: String,
    password: String,
    timeout: Duration,
) -> Result<String> {
    assert_eq!(method.method_type(), LoginMethodType::Account);
    unsafe {
        let acc = &*(data_ptr_of(method) as *const AccountLoginMethod);
        acc.start_login(username, password, timeout)
    }
}

pub fn perform_code_login(
    method: &dyn LoginMethod,
    account: String,
    callback: Box<dyn CodeLoginCallback>,
    timeout: Duration,
) -> Result<String> {
    assert_eq!(method.method_type(), LoginMethodType::Code);
    unsafe {
        let code = &*(data_ptr_of(method) as *const CodeLoginMethod);
        code.start_login(account, callback, timeout)
    }
}
