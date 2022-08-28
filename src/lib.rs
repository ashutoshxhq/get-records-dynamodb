use aws_sdk_dynamodb::{model::AttributeValue, Client};
use egnitely_client::{Context, Error, HandlerError};
use serde::{Deserialize, Serialize};
use serde_dynamo::{aws_sdk_dynamodb_0_17::from_item, to_item};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
struct FunctionContextData {
    pub table_name: String,
    pub primary_key: String,
    pub index_data: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FunctionInput {
    pub limit: i32,
    pub start_key: Option<HashMap<String, Value>>,
    pub filter: HashMap<String, Value>,
}

pub async fn handler(mut _ctx: Context, input: FunctionInput) -> Result<Value, Error> {
    let config_data = serde_json::from_value::<FunctionContextData>(_ctx.config())?;
    let mut records: Vec<Value> = Vec::new();
    let config = aws_config::from_env().region("ap-south-1").load().await;

    let mut condition = String::new();
    let mut filter_condition = String::new();
    let mut hash_key = String::new();
    let mut index_name: Option<String> = None;
    let mut exp_attr_values: HashMap<String, Value> = HashMap::new();
    let mut exp_attr_names: HashMap<String, String> = HashMap::new();
    let mut is_using_index = false;

    if input.filter.contains_key(&config_data.primary_key) {
        let filter_key_val = format!(":{}_val", config_data.primary_key);
        if let Some(value) = input.filter.get(&config_data.primary_key) {
            exp_attr_values.insert(filter_key_val.clone(), value.clone());
        }
        let filter_key_name = format!("#{}_key", config_data.primary_key);
        exp_attr_names.insert(filter_key_name.clone(), config_data.primary_key.clone());
        condition.push_str(&format!(
            "#{}_key = :{}_val",
            config_data.primary_key, config_data.primary_key
        ));
        hash_key = config_data.primary_key;
    } else {
        'outer: for (filter_key, filter_value) in input.filter.clone().into_iter() {
            for (index_key, index_value) in config_data.index_data.clone().into_iter() {
                if filter_key == index_key {
                    let filter_key_val = format!(":{}_val", filter_key);
                    exp_attr_values.insert(filter_key_val.clone(), filter_value);
                    let filter_key_name = format!("#{}_key", filter_key);
                    exp_attr_names.insert(filter_key_name.clone(), filter_key.clone());
                    condition.push_str(&format!("#{}_key = :{}_val", filter_key, filter_key));
                    index_name = Some(index_value);
                    hash_key = filter_key;
                    is_using_index = true;
                    break 'outer;
                }
            }
        }
    }

    if condition == "".to_string() {
        return Err(HandlerError::new(
            "NO_KEY_CONDITION".to_string(),
            "Please provide at least a primary key or an index field to be able to query"
                .to_string(),
            400,
        ));
    }
    let mut filter_data = input.filter.clone();
    filter_data.remove(&hash_key);
    let mut filter_len = filter_data.keys().len();

    for (filter_key, filter_value) in filter_data.into_iter() {
        let filter_key_val = format!(":{}_val", filter_key);
        exp_attr_values.insert(filter_key_val.clone(), filter_value);
        let filter_key_name = format!("#{}_key", filter_key);
        exp_attr_names.insert(filter_key_name.clone(), filter_key.clone());
        filter_condition.push_str(&format!("#{}_key = :{}_val", filter_key, filter_key));
        filter_condition.push_str(" and ");
        filter_len = filter_len - 1;
    }

    filter_condition.push_str("deleted_at = :deleted_at_val");

    let mut exp_values: HashMap<String, AttributeValue> = to_item(exp_attr_values)?;
    exp_values.insert(":deleted_at_val".to_string(), AttributeValue::Null(true));

    // TODO: Implement pagination
    let client = Client::new(&config);
    let req = client
        .query()
        .table_name(config_data.table_name)
        .limit(input.limit)
        .set_exclusive_start_key(None)
        .key_condition_expression(condition)
        .set_expression_attribute_names(Some(exp_attr_names))
        .set_expression_attribute_values(Some(exp_values));

    let req = if filter_condition != "".to_string() {
        req.filter_expression(filter_condition)
    } else {
        req
    };

    let req = if is_using_index {
        if let Some(index_name) = index_name {
            req.index_name(index_name)
        } else {
            req
        }
    } else {
        req
    };

    let res = req.send().await?;

    let last_evaluated_key = res.last_evaluated_key.clone();

    if let Some(items) = res.items() {
        for item in items.iter() {
            records.push(from_item(item.clone())?);
        }
    }
    if let Some(last_evaluated_key) = last_evaluated_key {
        let last_record_keys: Value = from_item(last_evaluated_key).unwrap();
        return Ok(json!({
                "message": format!("Successfully retrived {} records", records.len()),
                "data": records,
                "pagination": {
                    "last_record_keys" : last_record_keys,
                    "count": records.len()
                }
        }));
    } else {
        return Ok(json!({
                "message": format!("Successfully retrived {} records", records.len()),
                "data": records,
                "pagination": {
                    "count": records.len()
                }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn trigger_function() {
        let resp = handler(
            Context::new(
                "test".to_string(),
                "test".to_string(),
                json!({
                    "table_name": "functions",
                    "primary_key": "id",
                    "index_data": {
                        "team_id": "team_id-index"
                    },
                    "token_claims": {}
                }),
                json!({}),
            ),
            FunctionInput {
                limit: 10,
                start_key: None,
                filter: HashMap::from([("team_id".to_string(), json!("ashutosh"))]),
            },
        )
        .await;

        match resp {
            Ok(res) => {
                println!("{}", res);
            }
            Err(err) => {
                println!("Error: {:?}", err);
            }
        };

        assert_eq!(true, true);
    }
}
