use jni::{
    jni_str,
    objects::JValue,
    signature::RuntimeMethodSignature,
    Env,
};

#[derive(Clone, Copy)]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

pub fn log_java(env: &mut Env<'_>, level: LogLevel, message: impl ToString) {
    let Ok(log_manager) = env.find_class(jni_str!("org/apache/logging/log4j/LogManager")) else {
        eprintln!("[CROW] {}", message.to_string());
        return;
    };
    let get_logger =
        RuntimeMethodSignature::from_str("(Ljava/lang/String;)Lorg/apache/logging/log4j/Logger;")
            .unwrap();
    let engine_name = match env.new_string("CrowEngine") {
        Ok(s) => s,
        Err(_) => return,
    };
    let Ok(logger_obj) = env
        .call_static_method(
            log_manager,
            jni_str!("getLogger"),
            get_logger.method_signature(),
            &[JValue::Object(&engine_name)],
        )
        .and_then(|v| v.l())
    else {
        return;
    };
    let method = match level {
        LogLevel::Info => jni_str!("info"),
        LogLevel::Warn => jni_str!("warn"),
        LogLevel::Error => jni_str!("error"),
    };
    let sig = RuntimeMethodSignature::from_str("(Ljava/lang/String;)V").unwrap();
    let sig = sig.method_signature();
    if let Ok(j_msg) = env.new_string(message.to_string()) {
        let _ = env.call_method(logger_obj, method, sig, &[JValue::Object(&j_msg)]);
    }
}

pub fn clogger(env: &mut Env<'_>, message: impl ToString) {
    log_java(env, LogLevel::Info, message);
}

pub fn clogger_warn(env: &mut Env<'_>, message: impl ToString) {
    log_java(env, LogLevel::Warn, message);
}

pub fn clogger_err(env: &mut Env<'_>, message: impl ToString) {
    log_java(env, LogLevel::Error, message);
}