use jsonschema::Validator;
use serde_json::Value;

use crate::error::AppError;

/// Validates that the schema is a valid JSON Schema for structured output.
/// Requirements:
/// - Must be an object type at root
/// - Must have "properties" defined
pub fn validate_structured_schema(schema: &Value) -> Result<(), AppError> {
    let obj = schema
        .as_object()
        .ok_or_else(|| AppError::InvalidSchema("schema must be an object".into()))?;

    // Check type is "object"
    match obj.get("type") {
        Some(Value::String(t)) if t == "object" => {}
        Some(_) => {
            return Err(AppError::InvalidSchema(
                "schema type must be \"object\"".into(),
            ))
        }
        None => {
            return Err(AppError::InvalidSchema(
                "schema must have \"type\": \"object\"".into(),
            ))
        }
    }

    // Check properties exists and is an object
    match obj.get("properties") {
        Some(Value::Object(_)) => {}
        Some(_) => {
            return Err(AppError::InvalidSchema(
                "properties must be an object".into(),
            ))
        }
        None => {
            return Err(AppError::InvalidSchema(
                "schema must have \"properties\"".into(),
            ))
        }
    }

    // Validate it's a valid JSON Schema by trying to compile it
    Validator::new(schema)
        .map_err(|e| AppError::InvalidSchema(format!("invalid JSON Schema: {e}")))?;

    Ok(())
}

/// Validates that the output conforms to the schema.
pub fn validate_output(schema: &Value, output: &Value) -> Result<(), AppError> {
    let validator = Validator::new(schema)
        .map_err(|e| AppError::InvalidSchema(format!("invalid JSON Schema: {e}")))?;

    if validator.is_valid(output) {
        Ok(())
    } else {
        let errors: Vec<String> = validator
            .iter_errors(output)
            .map(|e| format!("{} at {}", e, e.instance_path()))
            .collect();

        Err(AppError::OutputValidation {
            errors,
            output: output.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_structured_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        assert!(validate_structured_schema(&schema).is_ok());
    }

    #[test]
    fn test_schema_missing_type() {
        let schema = json!({
            "properties": {
                "name": { "type": "string" }
            }
        });
        let err = validate_structured_schema(&schema).unwrap_err();
        assert!(err.to_string().contains("type"));
    }

    #[test]
    fn test_schema_wrong_type() {
        let schema = json!({
            "type": "array",
            "items": { "type": "string" }
        });
        let err = validate_structured_schema(&schema).unwrap_err();
        assert!(err.to_string().contains("object"));
    }

    #[test]
    fn test_schema_missing_properties() {
        let schema = json!({
            "type": "object"
        });
        let err = validate_structured_schema(&schema).unwrap_err();
        assert!(err.to_string().contains("properties"));
    }

    #[test]
    fn test_output_validation_success() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let output = json!({ "name": "test" });
        assert!(validate_output(&schema, &output).is_ok());
    }

    #[test]
    fn test_output_validation_failure() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let output = json!({ "name": 123 });
        assert!(validate_output(&schema, &output).is_err());
    }

    #[test]
    fn test_output_validation_missing_required() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let output = json!({});
        assert!(validate_output(&schema, &output).is_err());
    }
}
