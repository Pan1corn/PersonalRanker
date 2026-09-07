use std::{cmp::Ordering, collections::BTreeMap};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rusqlite::{params, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    domain::{
        import::ImportedFieldType,
        sort_task::{
            CreateSortTask, DragWorkspace, InitialOrder, RankedGroup, RankedItem, SortDirection,
            SortDisplayField, SortTaskMode, SortTaskOverview, SortTaskStatus,
        },
    },
    error::AppError,
    repository::{
        group_repository::{restore_structural_history, StructuralHistoryPayload},
        project_repository::ProjectRepository,
    },
};

#[derive(Debug)]
struct ItemForOrdering {
    id: String,
    original_index: usize,
    fields: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovePayload {
    group_id: String,
    from_position: usize,
    to_position: usize,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RankRangePayload {
    group_ids: Vec<String>,
}

#[derive(Debug)]
struct CopiedRankGroup {
    name: Option<String>,
    item_ids: Vec<String>,
}

impl ProjectRepository {
    pub fn create_sort_task(
        &mut self,
        request: &CreateSortTask,
        matrix_comparison_percent: Option<u8>,
    ) -> Result<SortTaskOverview, AppError> {
        let dataset_exists = self.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM datasets WHERE id = ?1)",
            [&request.dataset_id],
            |row| row.get::<_, bool>(0),
        )?;
        if !dataset_exists {
            return Err(AppError::InvalidInput("找不到要创建任务的数据集".into()));
        }
        let mut items = self.load_items_for_ordering(&request.dataset_id)?;
        if items.len() < 2 {
            return Err(AppError::InvalidInput(
                "数据集至少需要两个有效条目才能创建排序任务".into(),
            ));
        }
        if request.mode == SortTaskMode::Matrix && items.len() > 100 {
            return Err(AppError::InvalidInput(
                "1v1矩阵排序最多支持 100 个对象，请先缩小数据集".into(),
            ));
        }
        let matrix_comparison_percent = if request.mode == SortTaskMode::Matrix {
            let percent = matrix_comparison_percent.unwrap_or(100);
            if !(1..=100).contains(&percent) {
                return Err(AppError::InvalidInput(
                    "矩阵排序的随机抽取比例必须在 1% 到 100% 之间".into(),
                ));
            }
            Some(percent)
        } else {
            None
        };
        let mut copied_groups = None;
        let (order_type, order_field, order_direction, order_task_id) = match &request.initial_order
        {
            InitialOrder::Import => {
                items.sort_by_key(|item| item.original_index);
                ("import", None, None, None)
            }
            InitialOrder::Field {
                field_name,
                direction,
            } => {
                let field_type = self.load_orderable_field_type(&request.dataset_id, field_name)?;
                sort_items_by_field(&mut items, field_name, field_type, *direction);
                (
                    "field",
                    Some(field_name.as_str()),
                    Some(direction.as_str()),
                    None,
                )
            }
            InitialOrder::TaskResult { task_id } => {
                copied_groups =
                    Some(self.load_confirmed_task_groups(&request.dataset_id, task_id)?);
                ("task_result", None, None, Some(task_id.as_str()))
            }
        };

        let task_id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        transaction.execute(
            "INSERT INTO sort_tasks (id, dataset_id, name, criteria, mode, status, created_at, updated_at, initial_order_type, initial_order_field, initial_order_direction, initial_order_task_id, matrix_comparison_percent) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'draft', ?6, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                task_id,
                request.dataset_id,
                request.name,
                request.criteria,
                request.mode.as_str(),
                now,
                order_type,
                order_field,
                order_direction,
                order_task_id,
                matrix_comparison_percent,
            ],
        )?;
        if let Some(groups) = copied_groups {
            if matches!(request.mode, SortTaskMode::Matrix | SortTaskMode::Slider) {
                for (position, item_id) in groups
                    .iter()
                    .flat_map(|group| group.item_ids.iter())
                    .enumerate()
                {
                    let group_id = Uuid::new_v4().to_string();
                    transaction.execute(
                        "INSERT INTO rank_groups (id, sort_task_id, position, initial_position, created_at) VALUES (?1, ?2, ?3, ?3, ?4)",
                        params![group_id, task_id, position, now],
                    )?;
                    transaction.execute(
                        "INSERT INTO rank_group_items (rank_group_id, item_id, item_order) VALUES (?1, ?2, 0)",
                        params![group_id, item_id],
                    )?;
                }
            } else {
                for (position, group) in groups.iter().enumerate() {
                    let group_id = Uuid::new_v4().to_string();
                    transaction.execute(
                        "INSERT INTO rank_groups (id, sort_task_id, name, position, initial_position, created_at) VALUES (?1, ?2, ?3, ?4, ?4, ?5)",
                        params![group_id, task_id, group.name, position, now],
                    )?;
                    for (item_order, item_id) in group.item_ids.iter().enumerate() {
                        transaction.execute(
                            "INSERT INTO rank_group_items (rank_group_id, item_id, item_order) VALUES (?1, ?2, ?3)",
                            params![group_id, item_id, item_order],
                        )?;
                    }
                }
            }
        } else {
            for (position, item) in items.iter().enumerate() {
                let group_id = Uuid::new_v4().to_string();
                transaction.execute(
                    "INSERT INTO rank_groups (id, sort_task_id, position, initial_position, created_at) VALUES (?1, ?2, ?3, ?3, ?4)",
                    params![group_id, task_id, position, now],
                )?;
                transaction.execute(
                    "INSERT INTO rank_group_items (rank_group_id, item_id, item_order) VALUES (?1, ?2, 0)",
                    params![group_id, item.id],
                )?;
            }
        }
        transaction.commit()?;
        self.load_sort_task(&task_id)
    }

    pub fn list_sort_tasks(&self) -> Result<Vec<SortTaskOverview>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT t.id, t.dataset_id, t.name, t.criteria, t.mode, t.status, \
                    t.initial_order_type, t.initial_order_field, t.initial_order_direction, t.initial_order_task_id, \
                    t.matrix_comparison_percent, COUNT(g.id), t.created_at \
             FROM sort_tasks t LEFT JOIN rank_groups g ON g.sort_task_id = t.id AND g.is_active = 1 \
             GROUP BY t.id ORDER BY t.created_at",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<u8>>(10)?,
                row.get::<_, usize>(11)?,
                row.get::<_, String>(12)?,
            ))
        })?;
        rows.map(|row| {
            let (
                id,
                dataset_id,
                name,
                criteria,
                mode,
                status,
                order_type,
                order_field,
                order_direction,
                order_task_id,
                matrix_comparison_percent,
                rank_group_count,
                created_at,
            ) = row?;
            Ok(SortTaskOverview {
                id,
                dataset_id,
                name,
                criteria,
                mode: SortTaskMode::from_storage(&mode).ok_or_else(|| {
                    AppError::InvalidProject(format!("未知的排序任务模式：{mode}"))
                })?,
                status: SortTaskStatus::from_storage(&status).ok_or_else(|| {
                    AppError::InvalidProject(format!("未知的排序任务状态：{status}"))
                })?,
                initial_order: parse_initial_order(
                    &order_type,
                    order_field,
                    order_direction,
                    order_task_id,
                )?,
                matrix_comparison_percent,
                rank_group_count,
                created_at,
            })
        })
        .collect()
    }

    pub fn delete_sort_task(&mut self, task_id: &str) -> Result<(), AppError> {
        let transaction = self.connection_mut().transaction()?;
        let deleted = transaction.execute("DELETE FROM sort_tasks WHERE id = ?1", [task_id])?;
        if deleted == 0 {
            return Err(AppError::InvalidInput("找不到要删除的排序任务".into()));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn update_sort_task_criteria(
        &mut self,
        task_id: &str,
        criteria: &str,
    ) -> Result<SortTaskOverview, AppError> {
        let updated = self.connection().execute(
            "UPDATE sort_tasks SET criteria = ?1, updated_at = ?2 WHERE id = ?3",
            params![criteria, Utc::now().to_rfc3339(), task_id],
        )?;
        if updated == 0 {
            return Err(AppError::InvalidInput("找不到要修改的排序任务".into()));
        }
        self.load_sort_task(task_id)
    }

    fn load_sort_task(&self, task_id: &str) -> Result<SortTaskOverview, AppError> {
        self.list_sort_tasks()?
            .into_iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| AppError::InvalidProject("创建后无法重新读取排序任务".into()))
    }

    fn load_confirmed_task_groups(
        &self,
        dataset_id: &str,
        task_id: &str,
    ) -> Result<Vec<CopiedRankGroup>, AppError> {
        let status = self
            .connection()
            .query_row(
                "SELECT status FROM sort_tasks WHERE id = ?1 AND dataset_id = ?2",
                params![task_id, dataset_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match status.as_deref() {
            Some("confirmed") => {}
            Some(_) => {
                return Err(AppError::InvalidInput(
                    "只能沿用已经确认并锁定的排序任务结果".into(),
                ));
            }
            None => {
                return Err(AppError::InvalidInput(
                    "找不到同一数据表中的来源排序任务".into(),
                ));
            }
        }

        let mut statement = self.connection().prepare(
            "SELECT rg.id, rg.name, rgi.item_id FROM rank_groups rg \
             JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id \
             WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 \
             ORDER BY rg.position, rgi.item_order",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        let mut groups = Vec::<CopiedRankGroup>::new();
        let mut current_id = String::new();
        for row in rows {
            let (group_id, name, item_id) = row?;
            if current_id != group_id {
                current_id.clone_from(&group_id);
                groups.push(CopiedRankGroup {
                    name,
                    item_ids: Vec::new(),
                });
            }
            if let Some(group) = groups.last_mut() {
                group.item_ids.push(item_id);
            }
        }
        let copied_item_count = groups
            .iter()
            .map(|group| group.item_ids.len())
            .sum::<usize>();
        let dataset_item_count = self.connection().query_row(
            "SELECT COUNT(*) FROM items WHERE dataset_id = ?1 AND is_valid = 1",
            [dataset_id],
            |row| row.get::<_, usize>(0),
        )?;
        if groups.is_empty() || copied_item_count != dataset_item_count {
            return Err(AppError::InvalidProject(
                "来源排序结果没有完整覆盖当前数据表".into(),
            ));
        }
        Ok(groups)
    }

    pub fn load_drag_workspace(&self, task_id: &str) -> Result<DragWorkspace, AppError> {
        let (task_name, criteria, status, mode, dataset_id) = self
            .connection()
            .query_row(
                "SELECT name, criteria, status, mode, dataset_id FROM sort_tasks WHERE id = ?1",
                [task_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到排序任务".into()))?;
        let mode = SortTaskMode::from_storage(&mode)
            .ok_or_else(|| AppError::InvalidProject("排序任务模式无效".into()))?;
        match mode {
            SortTaskMode::Matrix | SortTaskMode::Comparison
                if !self.comparison_task_is_complete(task_id)? =>
            {
                return Err(AppError::InvalidInput(
                    "请先完成 1v1 排序，再进入拖拽微调".into(),
                ));
            }
            SortTaskMode::Slider if !self.slider_task_is_complete(task_id)? => {
                return Err(AppError::InvalidInput(
                    "请先完成滑杆排序，再进入拖拽微调".into(),
                ));
            }
            _ => {}
        }
        let status = SortTaskStatus::from_storage(&status)
            .ok_or_else(|| AppError::InvalidProject("排序任务状态无效".into()))?;
        let field_definitions = self.load_display_fields(&dataset_id)?;
        let primary_name = field_definitions
            .iter()
            .find(|(_, primary, _)| *primary)
            .map(|(name, _, _)| name.as_str())
            .ok_or_else(|| AppError::InvalidProject("数据集缺少主标识字段".into()))?;
        let mut statement = self.connection().prepare(
            "SELECT rg.id, rg.name, i.id, rg.position, rg.initial_position, i.fields_json FROM rank_groups rg \
             JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id \
             JOIN items i ON i.id = rgi.item_id \
             WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 ORDER BY rg.position, rgi.item_order",
        )?;
        let rows = statement.query_map([task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, usize>(3)?,
                row.get::<_, usize>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        let items = rows
            .map(|row| {
                let (group_id, _group_name, item_id, position, original_position, fields_json) =
                    row?;
                let fields: BTreeMap<String, Value> = serde_json::from_str(&fields_json)?;
                let display_fields = field_definitions
                    .iter()
                    .map(|(name, _, _)| SortDisplayField {
                        name: name.clone(),
                        value: fields.get(name).cloned().unwrap_or(Value::Null),
                    })
                    .collect::<Vec<_>>();
                let auxiliary_fields = field_definitions
                    .iter()
                    .filter(|(_, _, auxiliary)| *auxiliary)
                    .map(|(name, _, _)| SortDisplayField {
                        name: name.clone(),
                        value: fields.get(name).cloned().unwrap_or(Value::Null),
                    })
                    .collect();
                Ok(RankedItem {
                    group_id,
                    item_id,
                    position,
                    original_position,
                    primary_label: display_value(fields.get(primary_name)),
                    auxiliary_fields,
                    fields: display_fields,
                })
            })
            .collect::<Result<Vec<_>, AppError>>()?;
        let group_names = self.load_active_group_names(task_id)?;
        let mut occupied = 0;
        let groups = group_names
            .into_iter()
            .enumerate()
            .map(|(position, (group_id, name))| {
                let members = items
                    .iter()
                    .filter(|item| item.group_id == group_id)
                    .cloned()
                    .collect::<Vec<_>>();
                let starting_rank = occupied + 1;
                occupied += members.len();
                RankedGroup {
                    group_id,
                    name,
                    position,
                    starting_rank,
                    items: members,
                }
            })
            .collect();
        let has_comparisons = self.connection().query_row(
            "SELECT EXISTS(SELECT 1 FROM comparisons WHERE sort_task_id = ?1 AND is_undone = 0)",
            [task_id],
            |row| row.get::<_, bool>(0),
        )?;
        let (can_undo, can_redo) = self.history_state(task_id)?;
        Ok(DragWorkspace {
            task_id: task_id.to_owned(),
            task_name,
            criteria,
            status,
            items,
            groups,
            has_comparisons,
            can_undo,
            can_redo,
        })
    }

    pub fn move_rank_group(
        &mut self,
        task_id: &str,
        group_id: &str,
        to_position: usize,
    ) -> Result<DragWorkspace, AppError> {
        self.ensure_drag_task(task_id)?;
        let mut group_ids = self.ordered_group_ids(task_id)?;
        let from_position = group_ids
            .iter()
            .position(|id| id == group_id)
            .ok_or_else(|| AppError::InvalidInput("找不到要移动的条目".into()))?;
        if to_position >= group_ids.len() {
            return Err(AppError::InvalidInput("目标排名超出条目范围".into()));
        }
        if from_position == to_position {
            return self.load_drag_workspace(task_id);
        }
        let moved = group_ids.remove(from_position);
        group_ids.insert(to_position, moved);
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        transaction.execute(
            "DELETE FROM operation_logs WHERE sort_task_id = ?1 AND is_undone = 1",
            [task_id],
        )?;
        let sequence = transaction.query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM operation_logs WHERE sort_task_id = ?1",
            [task_id],
            |row| row.get::<_, i64>(0),
        )?;
        persist_group_order(&transaction, task_id, &group_ids)?;
        if let Some(state_json) = transaction
            .query_row(
                "SELECT state_json FROM comparison_sessions WHERE sort_task_id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
        {
            let mut state: crate::domain::comparison::ComparisonSessionState =
                serde_json::from_str(&state_json)?;
            if state.queue.is_empty() {
                state.sorted_group_ids.clone_from(&group_ids);
                transaction.execute(
                    "UPDATE comparison_sessions SET state_json = ?1, updated_at = ?2 WHERE sort_task_id = ?3",
                    params![serde_json::to_string(&state)?, now, task_id],
                )?;
            }
        }
        let forward = MovePayload {
            group_id: group_id.to_owned(),
            from_position,
            to_position,
        };
        let inverse = MovePayload {
            group_id: group_id.to_owned(),
            from_position: to_position,
            to_position: from_position,
        };
        transaction.execute(
            "INSERT INTO operation_logs (id, sort_task_id, operation_type, forward_payload_json, inverse_payload_json, sequence, is_undone, created_at) \
             VALUES (?1, ?2, 'move_rank_group', ?3, ?4, ?5, 0, ?6)",
            params![
                Uuid::new_v4().to_string(),
                task_id,
                serde_json::to_string(&forward)?,
                serde_json::to_string(&inverse)?,
                sequence,
                now,
            ],
        )?;
        transaction.execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        transaction.commit()?;
        self.load_drag_workspace(task_id)
    }

    pub fn apply_local_rank_order(
        &mut self,
        task_id: &str,
        start_position: usize,
        ordered_group_ids: &[String],
    ) -> Result<DragWorkspace, AppError> {
        self.ensure_drag_task(task_id)?;
        if ordered_group_ids.len() < 2 {
            return Err(AppError::InvalidInput(
                "局部重排至少需要两个连续排名组".into(),
            ));
        }
        let previous_order = self.ordered_group_ids(task_id)?;
        let end_position = start_position
            .checked_add(ordered_group_ids.len())
            .ok_or_else(|| AppError::InvalidInput("局部重排范围无效".into()))?;
        if end_position > previous_order.len() {
            return Err(AppError::InvalidInput("局部重排范围超出完整列表".into()));
        }
        let current_slice = &previous_order[start_position..end_position];
        let mut expected = current_slice.to_vec();
        let mut received = ordered_group_ids.to_vec();
        expected.sort();
        received.sort();
        if expected != received {
            return Err(AppError::InvalidInput(
                "局部重排只能调整所选连续区间内的排名组".into(),
            ));
        }
        if current_slice == ordered_group_ids {
            return self.load_drag_workspace(task_id);
        }

        let mut next_order = previous_order.clone();
        next_order[start_position..end_position].clone_from_slice(ordered_group_ids);
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        transaction.execute(
            "DELETE FROM operation_logs WHERE sort_task_id = ?1 AND is_undone = 1",
            [task_id],
        )?;
        let sequence = transaction.query_row(
            "SELECT COALESCE(MAX(sequence), 0) + 1 FROM operation_logs WHERE sort_task_id = ?1",
            [task_id],
            |row| row.get::<_, i64>(0),
        )?;
        persist_group_order(&transaction, task_id, &next_order)?;
        transaction.execute(
            "INSERT INTO operation_logs (id, sort_task_id, operation_type, forward_payload_json, inverse_payload_json, sequence, is_undone, created_at) \
             VALUES (?1, ?2, 'reorder_rank_range', ?3, ?4, ?5, 0, ?6)",
            params![
                Uuid::new_v4().to_string(),
                task_id,
                serde_json::to_string(&RankRangePayload { group_ids: next_order })?,
                serde_json::to_string(&RankRangePayload { group_ids: previous_order })?,
                sequence,
                now,
            ],
        )?;
        transaction.execute(
            "UPDATE sort_tasks SET status = 'sorting', updated_at = ?1 WHERE id = ?2",
            params![now, task_id],
        )?;
        transaction.commit()?;
        self.load_drag_workspace(task_id)
    }

    pub fn undo_drag_operation(&mut self, task_id: &str) -> Result<DragWorkspace, AppError> {
        self.apply_history_operation(task_id, true)
    }

    pub fn redo_drag_operation(&mut self, task_id: &str) -> Result<DragWorkspace, AppError> {
        self.apply_history_operation(task_id, false)
    }

    fn apply_history_operation(
        &mut self,
        task_id: &str,
        undo: bool,
    ) -> Result<DragWorkspace, AppError> {
        self.ensure_drag_task(task_id)?;
        let sql = if undo {
            "SELECT id, operation_type, inverse_payload_json FROM operation_logs WHERE sort_task_id = ?1 AND is_undone = 0 ORDER BY sequence DESC LIMIT 1"
        } else {
            "SELECT id, operation_type, forward_payload_json FROM operation_logs WHERE sort_task_id = ?1 AND is_undone = 1 ORDER BY sequence ASC LIMIT 1"
        };
        let operation = self
            .connection()
            .query_row(sql, [task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .optional()?;
        let Some((operation_id, operation_type, payload_json)) = operation else {
            return self.load_drag_workspace(task_id);
        };
        enum HistoryAction {
            Move(Vec<String>),
            Order(Vec<String>),
            Structural(StructuralHistoryPayload),
        }
        let action = if operation_type == "move_rank_group" {
            let payload: MovePayload = serde_json::from_str(&payload_json)?;
            let mut group_ids = self.ordered_group_ids(task_id)?;
            let current_position = group_ids
                .iter()
                .position(|id| id == &payload.group_id)
                .ok_or_else(|| AppError::InvalidProject("历史操作引用的排序组不存在".into()))?;
            if payload.to_position >= group_ids.len() {
                return Err(AppError::InvalidProject("历史操作中的目标排名无效".into()));
            }
            let moved = group_ids.remove(current_position);
            group_ids.insert(payload.to_position, moved);
            HistoryAction::Move(group_ids)
        } else if operation_type == "reorder_rank_range" {
            let payload: RankRangePayload = serde_json::from_str(&payload_json)?;
            let current_ids = self.ordered_group_ids(task_id)?;
            let mut expected = current_ids.clone();
            let mut received = payload.group_ids.clone();
            expected.sort();
            received.sort();
            if expected != received {
                return Err(AppError::InvalidProject(
                    "局部重排历史与当前排名组不一致".into(),
                ));
            }
            HistoryAction::Order(payload.group_ids)
        } else if matches!(
            operation_type.as_str(),
            "merge_rank_groups" | "split_rank_group_item"
        ) {
            HistoryAction::Structural(serde_json::from_str(&payload_json)?)
        } else {
            return Err(AppError::InvalidProject(format!(
                "不支持的历史操作类型：{operation_type}"
            )));
        };
        let now = Utc::now().to_rfc3339();
        let transaction = self.connection_mut().transaction()?;
        match action {
            HistoryAction::Move(group_ids) => {
                persist_group_order(&transaction, task_id, &group_ids)?;
            }
            HistoryAction::Order(group_ids) => {
                persist_group_order(&transaction, task_id, &group_ids)?;
            }
            HistoryAction::Structural(payload) => {
                restore_structural_history(&transaction, task_id, &payload, &now)?;
            }
        }
        transaction.execute(
            "UPDATE operation_logs SET is_undone = ?1 WHERE id = ?2",
            params![undo, operation_id],
        )?;
        transaction.commit()?;
        self.load_drag_workspace(task_id)
    }

    fn ensure_drag_task(&self, task_id: &str) -> Result<(), AppError> {
        let (mode, status) = self
            .connection()
            .query_row(
                "SELECT mode, status FROM sort_tasks WHERE id = ?1",
                [task_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到排序任务".into()))?;
        let mode = SortTaskMode::from_storage(&mode)
            .ok_or_else(|| AppError::InvalidProject("排序任务模式无效".into()))?;
        match mode {
            SortTaskMode::Matrix | SortTaskMode::Comparison
                if !self.comparison_task_is_complete(task_id)? =>
            {
                return Err(AppError::InvalidInput(
                    "请先完成 1v1 排序，再进行拖拽微调".into(),
                ));
            }
            SortTaskMode::Slider if !self.slider_task_is_complete(task_id)? => {
                return Err(AppError::InvalidInput(
                    "请先完成滑杆排序，再进行拖拽微调".into(),
                ));
            }
            _ => {}
        }
        if status == "confirmed" {
            return Err(AppError::InvalidInput(
                "结果已确认并锁定，请先在结果页解锁后再编辑".into(),
            ));
        }
        Ok(())
    }

    pub(super) fn ordered_group_ids(&self, task_id: &str) -> Result<Vec<String>, AppError> {
        let mut statement = self
            .connection()
            .prepare("SELECT id FROM rank_groups WHERE sort_task_id = ?1 AND is_active = 1 ORDER BY position")?;
        let rows = statement.query_map([task_id], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn load_active_group_names(
        &self,
        task_id: &str,
    ) -> Result<Vec<(String, Option<String>)>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT id, name FROM rank_groups WHERE sort_task_id = ?1 AND is_active = 1 ORDER BY position",
        )?;
        let rows = statement.query_map([task_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn comparison_task_is_complete(&self, task_id: &str) -> Result<bool, AppError> {
        if self.sort_task_mode(task_id)? == SortTaskMode::Matrix {
            let state_json = self
                .connection()
                .query_row(
                    "SELECT state_json FROM matrix_sessions WHERE sort_task_id = ?1",
                    [task_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let Some(state_json) = state_json else {
                return Ok(false);
            };
            let state: crate::domain::matrix::MatrixSessionState =
                serde_json::from_str(&state_json)?;
            return Ok(state.completed());
        }
        let state_json = self
            .connection()
            .query_row(
                "SELECT state_json FROM comparison_sessions WHERE sort_task_id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(state_json) = state_json else {
            return Ok(false);
        };
        let state: crate::domain::comparison::ComparisonSessionState =
            serde_json::from_str(&state_json)?;
        Ok(state.queue.is_empty())
    }

    pub(crate) fn sort_task_mode(&self, task_id: &str) -> Result<SortTaskMode, AppError> {
        let mode = self
            .connection()
            .query_row(
                "SELECT mode FROM sort_tasks WHERE id = ?1",
                [task_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到排序任务".into()))?;
        SortTaskMode::from_storage(&mode)
            .ok_or_else(|| AppError::InvalidProject(format!("未知的排序任务模式：{mode}")))
    }

    pub(super) fn load_display_fields(
        &self,
        dataset_id: &str,
    ) -> Result<Vec<(String, bool, bool)>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT name, is_primary_identifier, is_auxiliary_identifier FROM field_definitions \
             WHERE dataset_id = ?1 ORDER BY display_order",
        )?;
        let rows = statement.query_map([dataset_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn history_state(&self, task_id: &str) -> Result<(bool, bool), AppError> {
        self.connection()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM operation_logs WHERE sort_task_id = ?1 AND is_undone = 0), \
                        EXISTS(SELECT 1 FROM operation_logs WHERE sort_task_id = ?1 AND is_undone = 1)",
                [task_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(Into::into)
    }

    fn load_items_for_ordering(&self, dataset_id: &str) -> Result<Vec<ItemForOrdering>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT id, original_index, fields_json FROM items \
             WHERE dataset_id = ?1 AND is_valid = 1 ORDER BY original_index",
        )?;
        let rows = statement.query_map([dataset_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, usize>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row| {
            let (id, original_index, fields_json) = row?;
            Ok(ItemForOrdering {
                id,
                original_index,
                fields: serde_json::from_str(&fields_json)?,
            })
        })
        .collect()
    }

    fn load_orderable_field_type(
        &self,
        dataset_id: &str,
        field_name: &str,
    ) -> Result<ImportedFieldType, AppError> {
        let stored_type = self
            .connection()
            .query_row(
                "SELECT field_type FROM field_definitions WHERE dataset_id = ?1 AND name = ?2",
                params![dataset_id, field_name],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| AppError::InvalidInput("找不到用于初始排序的字段".into()))?;
        let field_type = ImportedFieldType::from_storage(&stored_type)
            .ok_or_else(|| AppError::InvalidProject(format!("未知的字段类型：{stored_type}")))?;
        if !matches!(
            field_type,
            ImportedFieldType::Number | ImportedFieldType::Date
        ) {
            return Err(AppError::InvalidInput(
                "初始排序字段必须是数字或日期类型".into(),
            ));
        }
        Ok(field_type)
    }

    #[cfg(test)]
    pub fn ranked_item_ids(&self, task_id: &str) -> Result<Vec<String>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT rgi.item_id FROM rank_groups rg \
             JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id \
             WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 ORDER BY rg.position, rgi.item_order",
        )?;
        let rows = statement.query_map([task_id], |row| row.get(0))?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    #[cfg(test)]
    pub fn ranked_field_values(
        &self,
        task_id: &str,
        field_name: &str,
    ) -> Result<Vec<Value>, AppError> {
        let mut statement = self.connection().prepare(
            "SELECT i.fields_json FROM rank_groups rg \
             JOIN rank_group_items rgi ON rgi.rank_group_id = rg.id \
             JOIN items i ON i.id = rgi.item_id \
             WHERE rg.sort_task_id = ?1 AND rg.is_active = 1 ORDER BY rg.position, rgi.item_order",
        )?;
        let rows = statement.query_map([task_id], |row| row.get::<_, String>(0))?;
        rows.map(|row| {
            let fields: BTreeMap<String, Value> = serde_json::from_str(&row?)?;
            Ok(fields.get(field_name).cloned().unwrap_or(Value::Null))
        })
        .collect()
    }
}

pub(super) fn persist_group_order(
    transaction: &Transaction<'_>,
    task_id: &str,
    group_ids: &[String],
) -> Result<(), AppError> {
    let offset = transaction.query_row(
        "SELECT COALESCE(MAX(position), -1) + ?1 + 1 FROM rank_groups WHERE sort_task_id = ?2",
        params![group_ids.len(), task_id],
        |row| row.get::<_, usize>(0),
    )?;
    transaction.execute(
        "UPDATE rank_groups SET position = position + ?1 WHERE sort_task_id = ?2",
        params![offset, task_id],
    )?;
    for (position, group_id) in group_ids.iter().enumerate() {
        transaction.execute(
            "UPDATE rank_groups SET position = ?1 WHERE id = ?2 AND sort_task_id = ?3",
            params![position, group_id, task_id],
        )?;
    }
    Ok(())
}

pub(super) fn display_value(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "—".into(),
        Some(Value::String(text)) => text.clone(),
        Some(value) => value.to_string(),
    }
}

fn parse_initial_order(
    order_type: &str,
    field: Option<String>,
    direction: Option<String>,
    task_id: Option<String>,
) -> Result<InitialOrder, AppError> {
    match order_type {
        "import" => Ok(InitialOrder::Import),
        "field" => Ok(InitialOrder::Field {
            field_name: field
                .ok_or_else(|| AppError::InvalidProject("字段初排任务缺少字段名".into()))?,
            direction: SortDirection::from_storage(direction.as_deref().unwrap_or_default())
                .ok_or_else(|| AppError::InvalidProject("字段初排任务缺少有效方向".into()))?,
        }),
        "task_result" => Ok(InitialOrder::TaskResult {
            task_id: task_id
                .ok_or_else(|| AppError::InvalidProject("结果初排任务缺少来源排序任务".into()))?,
        }),
        _ => Err(AppError::InvalidProject(format!(
            "未知的初始排序类型：{order_type}"
        ))),
    }
}

fn sort_items_by_field(
    items: &mut [ItemForOrdering],
    field_name: &str,
    field_type: ImportedFieldType,
    direction: SortDirection,
) {
    items.sort_by(|left, right| {
        let left_value = sort_value(left.fields.get(field_name), field_type);
        let right_value = sort_value(right.fields.get(field_name), field_type);
        let value_order = match (left_value, right_value) {
            (Some(left), Some(right)) => {
                let order = left.total_cmp(&right);
                if direction == SortDirection::Descending {
                    order.reverse()
                } else {
                    order
                }
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        value_order.then_with(|| left.original_index.cmp(&right.original_index))
    });
}

fn sort_value(value: Option<&Value>, field_type: ImportedFieldType) -> Option<f64> {
    let value = value?;
    match field_type {
        ImportedFieldType::Number => value
            .as_f64()
            .or_else(|| value.as_str()?.trim().parse::<f64>().ok()),
        ImportedFieldType::Date => {
            let text = value.as_str()?.trim();
            DateTime::parse_from_rfc3339(text)
                .map(|date| date.timestamp_millis() as f64)
                .or_else(|_| {
                    NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
                        .map(|date| date.and_utc().timestamp_millis() as f64)
                })
                .or_else(|_| {
                    NaiveDate::parse_from_str(text, "%Y-%m-%d")
                        .map(|date| date.and_hms_opt(0, 0, 0).expect("midnight is valid"))
                        .map(|date| date.and_utc().timestamp_millis() as f64)
                })
                .or_else(|_| {
                    NaiveDate::parse_from_str(text, "%Y/%m/%d")
                        .map(|date| date.and_hms_opt(0, 0, 0).expect("midnight is valid"))
                        .map(|date| date.and_utc().timestamp_millis() as f64)
                })
                .ok()
        }
        _ => None,
    }
}
