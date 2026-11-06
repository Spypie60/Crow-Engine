use anyhow::{Context, Result};
use jni::{
    jni_str,
    signature::FieldSignature,
    vm::ScopeToken,
    Env,
};

use crate::{error::jni_err, state::JVM};

pub fn with_env<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut Env<'_>) -> Result<R>,
{
    let jvm = JVM.get().context("[Crow Engine] JVM GLOBAL NOT FOUND")?;
    let mut scope = ScopeToken::default();
    let mut attachment = unsafe {
        jvm.get_env_attachment(&mut scope)
            .map_err(jni_err)
            .context("[Crow Engine] Bad day for getting Env")?
    };
    let env = attachment.borrow_env_mut();
    f(env)
}

pub fn crunning(env: &mut Env<'_>) -> bool {
    let Ok(client_class) = env.find_class(jni_str!("net/minecraft/client/Minecraft")) else {
        return false;
    };
    let sig = unsafe {
        FieldSignature::from_raw_parts(
            jni_str!("Lnet/minecraft/client/Minecraft;"),
            jni::signature::JavaType::Object,
        )
    };
    let instance = match env
        .get_static_field(client_class, jni_str!("instance"), sig)
        .and_then(|v| v.l())
    {
        Ok(obj) => obj,
        Err(_) => return false,
    };
    let sig = unsafe {
        FieldSignature::from_raw_parts(
            jni_str!("Z"),
            jni::signature::JavaType::Primitive(jni::signature::Primitive::Boolean),
        )
    };
    env.get_field(instance, jni_str!("running"), sig)
        .and_then(|v| v.z())
        .unwrap_or(false)
}