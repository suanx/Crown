//! 项目仓库：项目 CRUD 和侧栏摘要查询。

use chrono::Utc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::db::{Database, DbError};

/// 项目行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    /// 项目 ID。
    pub id: String,
    /// 显示名。
    pub name: String,
    /// 项目根目录。
    pub path: String,
    /// 创建时间，Unix 毫秒。
    pub created_at: i64,
    /// 更新时间，Unix 毫秒。
    pub updated_at: i64,
}

/// 项目列表摘要。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    /// 项目 ID。
    pub id: String,
    /// 显示名。
    pub name: String,
    /// 项目根目录。
    pub path: String,
    /// 关联的活跃顶层对话数。
    pub thread_count: u64,
    /// 最近使用时间，Unix 毫秒。
    pub last_used_at: i64,
}

/// 新建项目入参。
#[derive(Debug, Clone)]
pub struct ProjectInsert {
    /// 显示名。
    pub name: String,
    /// 项目根目录。
    pub path: String,
}

/// 更新项目入参。
#[derive(Debug, Default, Clone)]
pub struct ProjectUpdate {
    /// 新显示名。
    pub name: Option<String>,
    /// 新项目根目录。
    pub path: Option<String>,
}

/// 项目仓库。
pub struct ProjectRepo<'a> {
    db: &'a Database,
}

impl<'a> ProjectRepo<'a> {
    /// 创建仓库句柄。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 新建项目。
    pub fn create(&self, input: ProjectInsert) -> Result<Project, DbError> {
        let now = Utc::now().timestamp_millis();
        let project = Project {
            id: Ulid::new().to_string(),
            name: input.name,
            path: input.path,
            created_at: now,
            updated_at: now,
        };
        let conn = self.db.conn();
        conn.execute(
            "INSERT INTO projects (id, name, path, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                project.id,
                project.name,
                project.path,
                project.created_at,
                project.updated_at
            ],
        )?;
        Ok(project)
    }

    /// 列出项目摘要。
    pub fn list(&self) -> Result<Vec<ProjectSummary>, DbError> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.path,
                    (SELECT COUNT(*) FROM threads t
                     WHERE t.project_id = p.id
                       AND t.status = 'active'
                       AND t.parent_thread_id IS NULL) AS thread_count,
                    MAX(p.updated_at, COALESCE((
                        SELECT MAX(t.updated_at) FROM threads t
                        WHERE t.project_id = p.id
                          AND t.status = 'active'
                          AND t.parent_thread_id IS NULL
                    ), 0)) AS last_used_at
             FROM projects p
             ORDER BY last_used_at DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ProjectSummary {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    thread_count: row.get::<_, i64>(3)? as u64,
                    last_used_at: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// 获取单个项目。
    pub fn get(&self, id: &str) -> Result<Project, DbError> {
        let conn = self.db.conn();
        conn.query_row(
            "SELECT id, name, path, created_at, updated_at FROM projects WHERE id = ?1",
            params![id],
            |row| {
                Ok(Project {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .map_err(Into::into)
    }

    /// 更新项目。
    pub fn update(&self, id: &str, update: ProjectUpdate) -> Result<(), DbError> {
        let mut sets: Vec<&'static str> = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(name) = update.name {
            sets.push("name = ?");
            params_vec.push(Box::new(name));
        }
        if let Some(path) = update.path {
            sets.push("path = ?");
            params_vec.push(Box::new(path));
        }
        if sets.is_empty() {
            return Ok(());
        }
        sets.push("updated_at = ?");
        params_vec.push(Box::new(Utc::now().timestamp_millis()));
        let sql = format!("UPDATE projects SET {} WHERE id = ?", sets.join(", "));
        params_vec.push(Box::new(id.to_string()));
        let refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
        let conn = self.db.conn();
        conn.execute(&sql, refs.as_slice())?;
        Ok(())
    }

    /// 删除项目；关联对话保留并自动变成无项目。
    pub fn delete(&self, id: &str) -> Result<(), DbError> {
        let conn = self.db.conn();
        conn.execute(
            "UPDATE threads SET project_id = NULL WHERE project_id = ?1",
            params![id],
        )?;
        conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
        Ok(())
    }
}
