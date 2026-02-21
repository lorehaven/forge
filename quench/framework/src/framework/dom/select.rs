pub fn update_from_select(select_id: &str, update_fn: &str) -> String {
    format!(
        "const selected = document.getElementById('{select_id}').value;\
{update_fn}(selected);"
    )
}

pub fn set_select_value(select_id: &str, get_fn: &str) -> String {
    format!(
        "const select = document.getElementById('{select_id}');\
if (select) {{\
select.value = {get_fn}();\
}}"
    )
}
