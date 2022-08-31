use ::function_name::named;

use crate::image::Image;

#[test]
#[named]
fn can_successfully_retrieve_rate() {
    let image = Image::builder()
        .with_project_name(function_name!().to_string())
        .build();
}

#[test]
#[named]
fn can_successfully_retrieve_rate_2() {
    let image = Image::builder()
        .with_project_name(function_name!().to_string())
        .build();
}
