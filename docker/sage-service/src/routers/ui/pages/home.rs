use crate::clients::switchboard::{SwitchboardClient, VllmInstance};
use crate::routers::ui::common::{UiPageKind, render_page};
use actix_web::{HttpResponse, Responder, get, web};
use quench_srv::actix::routers::ui::pages::home::handle_home;
use quench_srv::prelude::{JwtConfig, with_base_path};
use quench_web::prelude::*;

#[get("/home")]
pub(super) async fn home(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
) -> impl Responder {
    let instances = switchboard.get_vllm_instances().await;
    handle_home(req, config, || render_home_page(instances)).await
}

#[get("/home/")]
pub(super) async fn home_slash(
    req: actix_web::HttpRequest,
    config: web::Data<JwtConfig>,
    switchboard: web::Data<SwitchboardClient>,
) -> impl Responder {
    let instances = switchboard.get_vllm_instances().await;
    handle_home(req, config, || render_home_page(instances)).await
}

fn render_home_page(instances_res: anyhow::Result<Vec<VllmInstance>>) -> HttpResponse {
    let mut model_select = select().class("model-selector").attr("id", "model-select");

    match instances_res {
        Ok(instances) => {
            if instances.is_empty() {
                model_select = model_select.child(
                    option()
                        .attr("value", "")
                        .attr("disabled", "disabled")
                        .attr("selected", "selected")
                        .text("No models available"),
                );
            } else {
                for instance in instances {
                    model_select = model_select
                        .child(option().attr("value", &instance.id).text(instance.model));
                }
            }
        }
        Err(err) => {
            tracing::error!("Failed to fetch models from switchboard: {}", err);
            model_select = model_select.child(
                option()
                    .attr("value", "")
                    .attr("disabled", "disabled")
                    .attr("selected", "selected")
                    .text("Switchboard unavailable"),
            );
        }
    }

    render_page(
        HttpResponse::Ok(),
        content().class("home-content").child(
            div()
                .class("chat-container")
                .child(
                    div()
                        .class("chat-history")
                        .child(div().class("chat-history-spacer"))
                        .child(
                            div()
                                .class("chat-message message-ai")
                                .attr("id", "msg-0")
                                .child(
                                    div().class("message-inner")
                                        .child(
                                            div().class("message-content")
                                                .attr("data-i18n", "ui_chat_welcome_message")
                                        )
                                )
                        ),
                )
                .child(
                    div()
                        .class("chat-navigation")
                        .child(
                            div()
                                .class("nav-dot active")
                                .attr("data-msg-id", "msg-0")
                                .child(
                                    div()
                                        .class("nav-tooltip")
                                        .text("Hello! I am Sage...")
                                )
                        )
                )
                .child(
                    form()
                        .attr("hx-post", with_base_path("/ui/chat/send"))
                        .attr("hx-target", ".chat-history")
                        .attr("hx-swap", "beforeend")
                        .attr("hx-on::after-request", "if(event.detail.successful) { document.getElementById('chat-input').value = ''; document.getElementById('chat-input').style.height = 'auto'; const history = document.querySelector('.chat-history'); history.scrollTop = history.scrollHeight; }")
                        .class("chat-input-wrapper")
                        .child(
                            div().class("chat-input-area-container")
                                .child(
                                    div().class("chat-input-area")
                                        .child(
                                            textarea()
                                                .attr("id", "chat-input")
                                                .attr("name", "message")
                                                .class("chat-input")
                                                .attr("rows", "1")
                                                .attr("data-i18n-placeholder", "ui_chat_input_placeholder")
                                                .attr("onkeydown", "if(event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); this.form.dispatchEvent(new Event('submit', {cancelable: true, bubbles: true})); }"),
                                        )
                                        .child(
                                            button()
                                                .attr("type", "submit")
                                                .class("chat-send-btn")
                                                .child(i().class("fas fa-arrow-up")),
                                        )
                                )
                                .child(
                                    div().class("chat-input-extras")
                                        .child(div().attr("style", "flex: 1;"))
                                        .child(model_select.attr("name", "instance_id"))
                                )
                        )
                ),
        ),
        UiPageKind::Home,
    )
}
