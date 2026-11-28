pub mod ci;
pub mod engine;
pub mod github;
pub mod image;
pub mod job;
pub mod log;
pub mod pipeline;
pub mod plan;
pub mod projects;
pub mod redact;
pub mod ship;
pub mod store;
pub mod text;
pub mod util;
pub mod walk;
pub mod watch;

#[cfg(test)]
mod ci_tests;
#[cfg(test)]
mod fake;
#[cfg(test)]
mod github_tests;
#[cfg(test)]
mod image_tests;
#[cfg(test)]
mod job_tests;
#[cfg(test)]
mod plan_cap_tests;
#[cfg(test)]
mod plan_error_tests;
#[cfg(test)]
mod plan_hardening_tests;
#[cfg(test)]
mod plan_image_tests;
#[cfg(test)]
mod plan_race_tests;
#[cfg(test)]
mod plan_tests;
#[cfg(test)]
mod projects_image_tests;
#[cfg(test)]
mod projects_tests;
#[cfg(test)]
mod secret_names_tests;
#[cfg(test)]
mod ship_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod text_tests;
#[cfg(test)]
mod walk_tests;
#[cfg(test)]
mod watch_tests;
