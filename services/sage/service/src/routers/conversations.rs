//! Conversations and projects.
//!
//! What the sidebar lists, the active branch of a conversation with its alternatives, and the changes the person makes to them
//! (delete, switch to another answer, edit a message, a new project).

use crate::clients::vllm::ChatMessage;
use crate::domain::models::{Conversation, Message, Project};
use crate::routers::files::Username;
use crate::routers::turns::owned_conversation;
use quench_core::uuid::Uuid;
use quench_db::prelude::{Crud, Db};
use quench_http::prelude::{Inject, Json, Path, Response, delete, get, http::StatusCode, post};
use sage_core::{
    ConversationItem, ConversationView, EditBody, FileInfo, MessageNode, NewProject, Source,
    SwitchBody,
};

fn json<T: quench_serde::Serialize>(status: StatusCode, value: &T) -> Response {
    Response::json(status, value)
        .unwrap_or_else(|_| Response::new(StatusCode::INTERNAL_SERVER_ERROR))
}

fn internal(err: impl std::fmt::Display) -> Response {
    quench_log::error!("Internal error: {}", err);
    Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal")
}

pub async fn get_conversation_messages(
    db: &quench_db::prelude::Db,
    active_message_id: Option<&str>,
) -> Result<Vec<ChatMessage>, quench_core::error::Error> {
    let mut chat_messages = Vec::new();
    let Some(mut current_id) = active_message_id.map(std::string::ToString::to_string) else {
        return Ok(chat_messages);
    };

    match db {
        quench_db::prelude::Db::Postgres(pg_db) => {
            let schema = quench_core::env::get_or("DB_SCHEMA", "sage");
            let table = format!("{schema}.messages");
            let query = format!(
                "WITH RECURSIVE thread AS (
                    SELECT id, parent_id, role, content, created_at, 0 as depth
                    FROM {table}
                    WHERE id = $1
                    UNION ALL
                    SELECT m.id, m.parent_id, m.role, m.content, m.created_at, t.depth + 1
                    FROM {table} m
                    INNER JOIN thread t ON t.parent_id = m.id
                )
                SELECT id, role, content FROM thread ORDER BY depth DESC"
            );

            let rows = crate::domain::rows::fetch_all_tuples::<(String, String, String)>(
                pg_db.pool(),
                &query,
                &[&current_id],
            )
            .await?;

            // So vision models keep seeing them in follow-up turns.
            let message_ids: Vec<String> = rows.iter().map(|(id, _, _)| id.clone()).collect();
            let mut images_by_message =
                crate::files::images::load_images_for_messages(db, &message_ids).await;

            for (id, role, content) in rows {
                chat_messages.push(ChatMessage {
                    role,
                    content,
                    tool_calls: None,
                    images: images_by_message.remove(&id),
                });
            }
        }
        quench_db::prelude::Db::InMemory(_mem_db) => {
            use quench_db::prelude::Crud;
            let repo = db.repository::<crate::domain::models::Message>();
            let mut visited = std::collections::HashSet::new();
            let mut message_list = Vec::new();
            while !current_id.is_empty() && visited.insert(current_id.clone()) {
                if let Ok(Some(msg)) = repo.read(&current_id).await {
                    message_list.push(msg.clone());
                    if let Some(pid) = msg.parent_id {
                        current_id = pid;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            message_list.reverse();
            for msg in message_list {
                chat_messages.push(ChatMessage {
                    role: msg.role,
                    content: msg.content,
                    tool_calls: None,
                    images: None,
                });
            }
        }
    }

    Ok(chat_messages)
}

pub async fn get_conversation_message_nodes(
    db: &quench_db::prelude::Db,
    active_message_id: Option<&str>,
) -> Result<Vec<crate::domain::models::Message>, quench_core::error::Error> {
    let mut message_nodes = Vec::new();
    let Some(mut current_id) = active_message_id.map(std::string::ToString::to_string) else {
        return Ok(message_nodes);
    };

    match db {
        quench_db::prelude::Db::Postgres(pg_db) => {
            let schema = quench_core::env::get_or("DB_SCHEMA", "sage");
            let table = format!("{schema}.messages");
            let query = format!(
                "WITH RECURSIVE thread AS (
                    SELECT id, conversation_id, parent_id, role, content, created_at, 0 as depth
                    FROM {table}
                    WHERE id = $1
                    UNION ALL
                    SELECT m.id, m.conversation_id, m.parent_id, m.role, m.content, m.created_at, t.depth + 1
                    FROM {table} m
                    INNER JOIN thread t ON t.parent_id = m.id
                )
                SELECT id, conversation_id, parent_id, role, content, created_at FROM thread ORDER BY depth DESC"
            );

            let rows = crate::domain::rows::fetch_all_tuples::<(
                String,
                String,
                Option<String>,
                String,
                String,
                String,
            )>(pg_db.pool(), &query, &[&current_id])
            .await?;

            for (id, conversation_id, parent_id, role, content, created_at) in rows {
                message_nodes.push(crate::domain::models::Message {
                    id,
                    conversation_id,
                    parent_id,
                    role,
                    content,
                    created_at,
                });
            }
        }
        quench_db::prelude::Db::InMemory(_mem_db) => {
            use quench_db::prelude::Crud;
            let repo = db.repository::<crate::domain::models::Message>();
            let mut visited = std::collections::HashSet::new();
            let mut list = Vec::new();
            while !current_id.is_empty() && visited.insert(current_id.clone()) {
                if let Ok(Some(msg)) = repo.read(&current_id).await {
                    list.push(msg.clone());
                    if let Some(pid) = msg.parent_id {
                        current_id = pid;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            list.reverse();
            message_nodes = list;
        }
    }

    Ok(message_nodes)
}

pub async fn get_siblings(
    db: &quench_db::prelude::Db,
    conversation_id: &str,
    parent_id: Option<&str>,
) -> Result<Vec<crate::domain::models::Message>, quench_core::error::Error> {
    match db {
        quench_db::prelude::Db::Postgres(pg_db) => {
            let schema = quench_core::env::get_or("DB_SCHEMA", "sage");
            let table = format!("{schema}.messages");
            let parent_filter = parent_id.map_or("parent_id IS NULL", |_| "parent_id = $2");
            let query = format!(
                "SELECT id, conversation_id, parent_id, role, content, created_at
                     FROM {table}
                     WHERE conversation_id = $1 AND {parent_filter}
                     ORDER BY created_at ASC"
            );

            let mut params: Vec<&dyn quench_pg::ToSql> = vec![&conversation_id];
            if let Some(pid) = &parent_id {
                params.push(pid);
            }
            let rows = crate::domain::rows::fetch_all_tuples::<(
                String,
                String,
                Option<String>,
                String,
                String,
                String,
            )>(pg_db.pool(), &query, &params)
            .await?;
            let mut siblings = Vec::new();
            for (id, conversation_id, parent_id, role, content, created_at) in rows {
                siblings.push(crate::domain::models::Message {
                    id,
                    conversation_id,
                    parent_id,
                    role,
                    content,
                    created_at,
                });
            }
            Ok(siblings)
        }
        quench_db::prelude::Db::InMemory(_mem_db) => {
            use quench_db::prelude::Crud;
            let repo = db.repository::<crate::domain::models::Message>();
            let all = repo.list().await?;
            let mut siblings: Vec<_> = all
                .into_iter()
                .filter(|m| {
                    m.conversation_id == conversation_id && m.parent_id.as_deref() == parent_id
                })
                .collect();
            siblings.sort_by(|a, b| a.created_at.cmp(&b.created_at));
            Ok(siblings)
        }
    }
}

pub async fn switch_active_message(
    db: &quench_db::prelude::Db,
    conversation_id: &str,
    target_message_id: &str,
) -> Result<(), quench_core::error::Error> {
    let mut current_id = target_message_id.to_string();

    match db {
        quench_db::prelude::Db::Postgres(pg_db) => {
            let schema = quench_core::env::get_or("DB_SCHEMA", "sage");
            let table = format!("{schema}.messages");

            loop {
                let query = format!(
                    "SELECT id FROM {table} WHERE conversation_id = $1 AND parent_id = $2 ORDER BY created_at DESC LIMIT 1"
                );
                let child_opt: Option<(String,)> = crate::domain::rows::fetch_optional_tuple(
                    pg_db.pool(),
                    &query,
                    &[&conversation_id, &current_id],
                )
                .await?;

                if let Some((child_id,)) = child_opt {
                    current_id = child_id;
                } else {
                    break;
                }
            }
        }
        quench_db::prelude::Db::InMemory(_mem_db) => {
            use quench_db::prelude::Crud;
            let repo = db.repository::<crate::domain::models::Message>();
            loop {
                let all = repo.list().await?;
                let mut children: Vec<_> = all
                    .into_iter()
                    .filter(|m| {
                        m.conversation_id == conversation_id
                            && m.parent_id.as_deref() == Some(&current_id)
                    })
                    .collect();
                children.sort_by(|a, b| b.created_at.cmp(&a.created_at));
                if let Some(child) = children.first() {
                    current_id.clone_from(&child.id);
                } else {
                    break;
                }
            }
        }
    }

    let conv_repo = db.repository::<crate::domain::models::Conversation>();
    if let Some(mut conv) = conv_repo.read(conversation_id).await? {
        conv.active_message_id = Some(current_id);
        conv.updated_at = quench_core::time::Time::now().to_rfc3339();
        conv_repo.update(&conv).await?;
    }

    Ok(())
}

#[get("/api/v1/conversations")]
pub async fn list_conversations(username: Username, Inject(db): Inject<Db>) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    let mut conversations = match db.repository::<Conversation>().list().await {
        Ok(list) => list,
        Err(err) => return internal(err),
    };
    conversations.retain(|c| c.owner == username);
    conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let items: Vec<ConversationItem> = conversations
        .into_iter()
        .map(|c| ConversationItem {
            id: c.id,
            title: c.title,
            project_id: c.project_id,
            updated_at: c.updated_at,
        })
        .collect();
    json(StatusCode::OK, &items)
}

fn file_info(file: crate::domain::models::File) -> FileInfo {
    FileInfo {
        id: file.id,
        file_name: file.file_name,
        mime_type: file.mime_type,
        file_size: file.file_size,
        conversation_id: file.conversation_id,
        project_id: file.project_id,
        message_id: file.message_id,
        status: file.status,
        error_message: file.error_message,
    }
}

/// The active branch of a conversation, each message with the alternatives it has, the sources of Sage's answers and the files of
/// the person's messages.
#[get("/api/v1/conversations/{id}")]
pub async fn get_conversation(
    username: Username,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    let conv = match owned_conversation(&db, &id, &username).await {
        Ok(Some(conv)) => conv,
        Ok(None) => {
            return Response::text(StatusCode::NOT_FOUND, "api_error_conversation_not_found");
        }
        Err(response) => return response,
    };

    let nodes = match get_conversation_message_nodes(&db, conv.active_message_id.as_deref()).await {
        Ok(nodes) => nodes,
        Err(err) => return internal(err),
    };
    let assistant_ids: Vec<String> = nodes
        .iter()
        .filter(|m| m.role == "assistant")
        .map(|m| m.id.clone())
        .collect();
    let user_ids: Vec<String> = nodes
        .iter()
        .filter(|m| m.role == "user")
        .map(|m| m.id.clone())
        .collect();
    let mut sources = crate::files::rag::load_sources_for_messages(&db, &assistant_ids).await;
    let mut attachments = crate::routers::files::files_by_message(&db, &user_ids).await;

    let mut messages = Vec::with_capacity(nodes.len());
    for node in nodes {
        let siblings = get_siblings(&db, &node.conversation_id, node.parent_id.as_deref())
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|s| s.id)
            .collect();
        messages.push(MessageNode {
            sources: sources
                .remove(&node.id)
                .unwrap_or_default()
                .into_iter()
                .map(|s| Source {
                    file_name: s.file_name,
                    chunk_index: s.chunk_index,
                    detail: s.detail,
                    similarity: s.similarity,
                })
                .collect(),
            attachments: attachments
                .remove(&node.id)
                .unwrap_or_default()
                .into_iter()
                .map(file_info)
                .collect(),
            id: node.id,
            parent_id: node.parent_id,
            role: node.role,
            content: node.content,
            created_at: node.created_at,
            siblings,
        });
    }
    json(
        StatusCode::OK,
        &ConversationView {
            id: conv.id,
            title: conv.title,
            project_id: conv.project_id,
            messages,
        },
    )
}

#[delete("/api/v1/conversations/{id}")]
pub async fn delete_conversation(
    username: Username,
    Path(id): Path<String>,
    Inject(db): Inject<Db>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    match owned_conversation(&db, &id, &username).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Response::text(StatusCode::NOT_FOUND, "api_error_conversation_not_found");
        }
        Err(response) => return response,
    }
    match db.repository::<Conversation>().delete(&id).await {
        Ok(()) => Response::new(StatusCode::NO_CONTENT),
        Err(err) => internal(err),
    }
}

/// Makes another alternative the conversation's current branch (and its newest continuation the tip).
#[post("/api/v1/conversations/{id}/switch")]
pub async fn switch_branch(
    username: Username,
    Path(id): Path<String>,
    Json(body): Json<SwitchBody>,
    Inject(db): Inject<Db>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    match owned_conversation(&db, &id, &username).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Response::text(StatusCode::NOT_FOUND, "api_error_conversation_not_found");
        }
        Err(response) => return response,
    }
    // The target has to be a message of this conversation.
    match db
        .repository::<Message>()
        .read(&body.target_message_id)
        .await
    {
        Ok(Some(target)) if target.conversation_id == id => {}
        Ok(_) => return Response::text(StatusCode::NOT_FOUND, "api_error_message_not_found"),
        Err(err) => return internal(err),
    }
    if let Err(err) = switch_active_message(&db, &id, &body.target_message_id).await {
        quench_log::error!("Failed to switch active branch: {}", err);
        return Response::text(StatusCode::INTERNAL_SERVER_ERROR, "api_error_internal");
    }
    Response::new(StatusCode::NO_CONTENT)
}

/// Another version of one of the person's messages: a sibling that becomes the tip. Sage answers it when the conversation is
/// opened (`resume`).
#[post("/api/v1/messages/{id}/edit")]
pub async fn edit_message(
    username: Username,
    Path(id): Path<String>,
    Json(body): Json<EditBody>,
    Inject(db): Inject<Db>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    let repo = db.repository::<Message>();
    let Ok(Some(msg)) = repo.read(&id).await else {
        return Response::text(StatusCode::NOT_FOUND, "api_error_message_not_found");
    };
    if msg.role != "user" {
        return Response::text(StatusCode::BAD_REQUEST, "api_error_edit_non_user");
    }
    let content = body.new_content.trim().to_string();
    if content.is_empty() {
        return Response::text(StatusCode::BAD_REQUEST, "api_error_empty_message");
    }
    let mut conv = match owned_conversation(&db, &msg.conversation_id, &username).await {
        Ok(Some(conv)) => conv,
        Ok(None) => {
            return Response::text(StatusCode::NOT_FOUND, "api_error_conversation_not_found");
        }
        Err(response) => return response,
    };

    let user_msg_id = Uuid::new_v4().to_string();
    let user_msg = Message {
        id: user_msg_id.clone(),
        conversation_id: msg.conversation_id,
        parent_id: msg.parent_id, // Branch from the same parent
        role: "user".to_string(),
        content,
        created_at: quench_core::time::Time::now().to_rfc3339(),
    };
    if let Err(err) = repo.create(&user_msg).await {
        return internal(err);
    }
    conv.active_message_id = Some(user_msg_id.clone());
    conv.updated_at = quench_core::time::Time::now().to_rfc3339();
    if let Err(err) = db.repository::<Conversation>().update(&conv).await {
        return internal(err);
    }
    json(StatusCode::OK, &sage_core::Created { id: user_msg_id })
}

#[get("/api/v1/projects")]
pub async fn list_projects(username: Username, Inject(db): Inject<Db>) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    let mut projects = match db.repository::<Project>().list().await {
        Ok(list) => list,
        Err(err) => return internal(err),
    };
    projects.retain(|p| p.owner == username);
    projects.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let items: Vec<sage_core::Project> = projects
        .into_iter()
        .map(|p| sage_core::Project {
            id: p.id,
            name: p.name,
            updated_at: p.updated_at,
        })
        .collect();
    json(StatusCode::OK, &items)
}

#[post("/api/v1/projects")]
pub async fn create_project(
    username: Username,
    Json(body): Json<NewProject>,
    Inject(db): Inject<Db>,
) -> Response {
    let username = match username.or_401() {
        Ok(username) => username,
        Err(response) => return response,
    };
    let draft = sage_core::ProjectDraft { name: body.name };
    let Ok(body) = draft.body() else {
        return Response::text(StatusCode::BAD_REQUEST, "api_error_project_name");
    };
    let now = quench_core::time::Time::now().to_rfc3339();
    let project = Project {
        id: Uuid::new_v4().to_string(),
        name: body.name,
        owner: username,
        created_at: now.clone(),
        updated_at: now,
    };
    if let Err(err) = db.repository::<Project>().create(&project).await {
        return internal(err);
    }
    json(
        StatusCode::CREATED,
        &sage_core::Project {
            id: project.id,
            name: project.name,
            updated_at: project.updated_at,
        },
    )
}

pub fn register_routes() {
    let _ = list_conversations as fn(_, _) -> _;
    let _ = get_conversation as fn(_, _, _) -> _;
    let _ = delete_conversation as fn(_, _, _) -> _;
    let _ = switch_branch as fn(_, _, _, _) -> _;
    let _ = edit_message as fn(_, _, _, _) -> _;
    let _ = list_projects as fn(_, _) -> _;
    let _ = create_project as fn(_, _, _) -> _;
}
