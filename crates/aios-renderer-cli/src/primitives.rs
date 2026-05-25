//! Primitive [`Renderable`](crate::Renderable) implementations.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::{OutputFormat, RenderContext, RenderError, Renderable};

impl Renderable for String {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        self.as_str().render(format, ctx)
    }
}

impl Renderable for &str {
    fn render(&self, format: OutputFormat, _ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text | OutputFormat::Tree | OutputFormat::Table => Ok((*self).to_owned()),
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
        }
    }
}

impl Renderable for i64 {
    fn render(&self, format: OutputFormat, _ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text | OutputFormat::Tree | OutputFormat::Table => Ok(self.to_string()),
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
        }
    }
}

impl Renderable for u64 {
    fn render(&self, format: OutputFormat, _ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text | OutputFormat::Tree | OutputFormat::Table => Ok(self.to_string()),
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
        }
    }
}

impl Renderable for bool {
    fn render(&self, format: OutputFormat, _ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text | OutputFormat::Tree | OutputFormat::Table => Ok(self.to_string()),
            OutputFormat::Json => serde_json::to_string(self)
                .map_err(|err| RenderError::SerializationFailed(err.to_string())),
        }
    }
}

impl<T: Renderable> Renderable for Option<T> {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match (self, format) {
            (Some(value), _) => value.render(format, ctx),
            (None, OutputFormat::Json) => Ok("null".to_owned()),
            (None, OutputFormat::Text | OutputFormat::Tree | OutputFormat::Table) => {
                Ok(String::new())
            }
        }
    }
}

impl<T: Renderable> Renderable for Vec<T> {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => self
                .iter()
                .map(|value| value.render(OutputFormat::Text, ctx))
                .collect::<Result<Vec<_>, _>>()
                .map(|values| values.join("\n")),
            OutputFormat::Json => render_json_array(self, ctx),
            OutputFormat::Tree => self
                .iter()
                .map(|value| {
                    value
                        .render(OutputFormat::Tree, ctx)
                        .map(|item| format!("- {item}"))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|values| values.join("\n")),
            OutputFormat::Table => {
                let rows = self
                    .iter()
                    .enumerate()
                    .map(|(idx, value)| {
                        value
                            .render(OutputFormat::Table, ctx)
                            .map(|item| format!("{idx} | {item}"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(table_with_header("index | value", rows))
            }
        }
    }
}

impl<T: Renderable> Renderable for BTreeMap<String, T> {
    fn render(&self, format: OutputFormat, ctx: &RenderContext) -> Result<String, RenderError> {
        match format {
            OutputFormat::Text => self
                .iter()
                .map(|(key, value)| {
                    value
                        .render(OutputFormat::Text, ctx)
                        .map(|rendered| format!("{key}: {rendered}"))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|values| values.join("\n")),
            OutputFormat::Json => render_json_object(self, ctx),
            OutputFormat::Tree => self
                .iter()
                .map(|(key, value)| {
                    value
                        .render(OutputFormat::Tree, ctx)
                        .map(|rendered| format!("{key}\n  {rendered}"))
                })
                .collect::<Result<Vec<_>, _>>()
                .map(|values| values.join("\n")),
            OutputFormat::Table => {
                let rows = self
                    .iter()
                    .map(|(key, value)| {
                        value
                            .render(OutputFormat::Table, ctx)
                            .map(|rendered| format!("{key} | {rendered}"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(table_with_header("key | value", rows))
            }
        }
    }
}

fn render_json_array<T: Renderable>(
    values: &[T],
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let values = values
        .iter()
        .map(|value| render_json_value(value, ctx))
        .collect::<Result<Vec<_>, _>>()?;

    serde_json::to_string(&values).map_err(|err| RenderError::SerializationFailed(err.to_string()))
}

fn render_json_object<T: Renderable>(
    values: &BTreeMap<String, T>,
    ctx: &RenderContext,
) -> Result<String, RenderError> {
    let mut object = Map::new();

    for (key, value) in values {
        object.insert(key.clone(), render_json_value(value, ctx)?);
    }

    serde_json::to_string(&Value::Object(object))
        .map_err(|err| RenderError::SerializationFailed(err.to_string()))
}

fn render_json_value<T: Renderable>(value: &T, ctx: &RenderContext) -> Result<Value, RenderError> {
    let rendered = value.render(OutputFormat::Json, ctx)?;

    serde_json::from_str(&rendered).map_err(|err| RenderError::SerializationFailed(err.to_string()))
}

fn table_with_header(header: &str, rows: Vec<String>) -> String {
    std::iter::once(header.to_owned())
        .chain(std::iter::once("--- | ---".to_owned()))
        .chain(rows)
        .collect::<Vec<_>>()
        .join("\n")
}
