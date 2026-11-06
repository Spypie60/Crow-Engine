use anyhow::anyhow;

pub fn jni_err(err: impl std::fmt::Debug) -> anyhow::Error {
    anyhow!("{err:?}")
}