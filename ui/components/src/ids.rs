//! Disjoint ComponentId namespaces across shared AgentKit Composition surfaces.
//! Non-overlapping IDs allow containers (e.g. `AgentRunContainer`) to mount multiple
//! surfaces into a single `ContentHost` without ID collisions.

use tekes_ui_controls::composition::ComponentId;

pub const RUN_CONTAINER_SHELL_ID: ComponentId = ComponentId(1);
pub const RUN_CONTAINER_STACK_ID: ComponentId = ComponentId(10);
pub const RUN_SETUP_TOOLBAR_ROOT_ID: ComponentId = ComponentId(20);
pub const RUN_SETUP_TOOLBAR_SEP_ID: ComponentId = ComponentId(21);

// Transcript surface: 100..=399
pub const TRANSCRIPT_ROOT_ID: ComponentId = ComponentId(100);
pub const TRANSCRIPT_HEADER_ID: ComponentId = ComponentId(101);
pub const TRANSCRIPT_STATUS_ID: ComponentId = ComponentId(102);
pub const TRANSCRIPT_EARLIER_BTN_ID: ComponentId = ComponentId(103);
pub const TRANSCRIPT_LATEST_BTN_ID: ComponentId = ComponentId(104);
pub const TRANSCRIPT_SEARCH_ROW_ID: ComponentId = ComponentId(105);
pub const TRANSCRIPT_SEARCH_ID: ComponentId = ComponentId(106);
pub const TRANSCRIPT_READER_ID: ComponentId = ComponentId(107);
pub const TRANSCRIPT_NOTICE_SLOT_ID: ComponentId = ComponentId(108);
pub const TRANSCRIPT_EARLIER_SLOT_ID: ComponentId = ComponentId(109);
pub const TRANSCRIPT_THINKING_DISC_ID: ComponentId = ComponentId(110);
pub const TRANSCRIPT_THINKING_TRIGGER_ID: ComponentId = ComponentId(111);
pub const TRANSCRIPT_THINKING_BODY_ID: ComponentId = ComponentId(112);
pub const TRANSCRIPT_NOTICE_PANEL_ID: ComponentId = ComponentId(113);
pub const TRANSCRIPT_NOTICE_TEXT_ID: ComponentId = ComponentId(114);
pub const TRANSCRIPT_RECONNECT_BTN_ID: ComponentId = ComponentId(115);
pub const TRANSCRIPT_LATEST_USER_SLOT_ID: ComponentId = ComponentId(116);
pub const TRANSCRIPT_USER_BUBBLE_ROW_ID: ComponentId = ComponentId(117);
pub const TRANSCRIPT_USER_BUBBLE_PANEL_ID: ComponentId = ComponentId(118);
pub const TRANSCRIPT_USER_BUBBLE_TEXT_ID: ComponentId = ComponentId(119);
pub const TRANSCRIPT_TOOL_DISC_ID: ComponentId = ComponentId(120);
pub const TRANSCRIPT_TOOL_TRIGGER_ID: ComponentId = ComponentId(121);
pub const TRANSCRIPT_TOOL_BODY_ID: ComponentId = ComponentId(122);
pub const TRANSCRIPT_LATEST_TURN_SLOT_ID: ComponentId = ComponentId(123);
pub const TRANSCRIPT_FOOTNOTE_ROW_ID: ComponentId = ComponentId(124);
pub const TRANSCRIPT_COPY_BTN_ID: ComponentId = ComponentId(125);
pub const TRANSCRIPT_COPY_ICON_ID: ComponentId = ComponentId(126);
pub const TRANSCRIPT_UP_BTN_ID: ComponentId = ComponentId(127);
pub const TRANSCRIPT_UP_ICON_ID: ComponentId = ComponentId(128);
pub const TRANSCRIPT_DOWN_BTN_ID: ComponentId = ComponentId(129);
pub const TRANSCRIPT_RICH_DISC_ID: ComponentId = ComponentId(130);
pub const TRANSCRIPT_RICH_TRIGGER_ID: ComponentId = ComponentId(131);
pub const TRANSCRIPT_RICH_SCROLL_ID: ComponentId = ComponentId(132);
pub const TRANSCRIPT_RICH_BODY_ID: ComponentId = ComponentId(133);
pub const TRANSCRIPT_DOWN_ICON_ID: ComponentId = ComponentId(134);
pub const TRANSCRIPT_FORK_BTN_ID: ComponentId = ComponentId(135);
pub const TRANSCRIPT_FORK_ICON_ID: ComponentId = ComponentId(136);
pub const TRANSCRIPT_LATEST_SLOT_ID: ComponentId = ComponentId(137);
pub const TRANSCRIPT_LATEST_ICON_ID: ComponentId = ComponentId(138);
pub const TRANSCRIPT_ACTIONABLES_STACK_ID: ComponentId = ComponentId(140);
pub const TRANSCRIPT_SPACER_ID: ComponentId = ComponentId(141);
pub const TRANSCRIPT_SCROLL_ID: ComponentId = ComponentId(142);
pub const TRANSCRIPT_SCROLL_COLUMN_ID: ComponentId = ComponentId(143);
pub const TRANSCRIPT_SCROLL_INNER_ID: ComponentId = ComponentId(144);
pub const TRANSCRIPT_TURNS_STACK_ID: ComponentId = ComponentId(145);
pub const TRANSCRIPT_EARLIER_ROW_ID: ComponentId = ComponentId(146);
pub const TRANSCRIPT_EMPTY_STATE_ID: ComponentId = ComponentId(147);
pub const TRANSCRIPT_ERROR_PANEL_ID: ComponentId = ComponentId(149);
pub const TRANSCRIPT_ACTIONABLE_BASE: u64 = 150;
pub const TRANSCRIPT_TURN_BASE: u64 = 200_000;
pub const TRANSCRIPT_TOOL_ROW_BASE: u64 = 400_000;

// Composer surface: 400..=599
pub const COMPOSER_ROOT_ID: ComponentId = ComponentId(400);
pub const COMPOSER_CHIPS_ROW_ID: ComponentId = ComponentId(401);
pub const COMPOSER_EDITOR_ID: ComponentId = ComponentId(402);
pub const COMPOSER_CONTROLS_STACK_ID: ComponentId = ComponentId(403);
pub const COMPOSER_PRIMARY_ROW_ID: ComponentId = ComponentId(404);
pub const COMPOSER_SECONDARY_ROW_ID: ComponentId = ComponentId(405);
pub const COMPOSER_QUEUED_SLOT_ID: ComponentId = ComponentId(406);
pub const COMPOSER_QUEUED_PANEL_ID: ComponentId = ComponentId(407);
pub const COMPOSER_QUEUED_ROW_ID: ComponentId = ComponentId(408);
pub const COMPOSER_QUEUED_TEXT_ID: ComponentId = ComponentId(409);
pub const COMPOSER_MODEL_SELECT_ID: ComponentId = ComponentId(410);
pub const COMPOSER_PRESET_SELECT_ID: ComponentId = ComponentId(411);
pub const COMPOSER_ATTACH_BTN_ID: ComponentId = ComponentId(412);
pub const COMPOSER_RECONNECT_BTN_ID: ComponentId = ComponentId(413);
pub const COMPOSER_CANCEL_BTN_ID: ComponentId = ComponentId(414);
pub const COMPOSER_SEND_BTN_ID: ComponentId = ComponentId(415);
pub const COMPOSER_CARD_ID: ComponentId = ComponentId(416);
pub const COMPOSER_CARD_STACK_ID: ComponentId = ComponentId(417);
pub const COMPOSER_HARNESS_LABEL_ID: ComponentId = ComponentId(418);
pub const COMPOSER_QUEUED_STEER_BTN_ID: ComponentId = ComponentId(419);
pub const COMPOSER_QUEUED_REMOVE_BTN_ID: ComponentId = ComponentId(420);
pub const COMPOSER_INPUT_ROW_ID: ComponentId = ComponentId(422);
pub const COMPOSER_EFFORT_SELECT_ID: ComponentId = ComponentId(423);
pub const COMPOSER_HARNESS_SELECT_ID: ComponentId = ComponentId(424);
pub const COMPOSER_CENTER_COLUMN_ID: ComponentId = ComponentId(425);
pub const COMPOSER_CHIP_BASE: u64 = 450;

// Sidebar surface: 600..=699 (+ passive rows 10_000+, row decor 50_000+)
pub const SIDEBAR_SHELL_ID: ComponentId = ComponentId(600);
pub const SIDEBAR_ROOT_ID: ComponentId = ComponentId(601);
pub const SIDEBAR_TOP_BAR_ID: ComponentId = ComponentId(602);
pub const SIDEBAR_NEW_BTN_ID: ComponentId = ComponentId(603);
pub const SIDEBAR_PIN_BTN_ID: ComponentId = ComponentId(604);
pub const SIDEBAR_ARCHIVE_BTN_ID: ComponentId = ComponentId(605);
pub const SIDEBAR_FILTER_ID: ComponentId = ComponentId(606);
pub const SIDEBAR_SEARCH_ID: ComponentId = ComponentId(607);
pub const SIDEBAR_LIST_SCROLL_ID: ComponentId = ComponentId(608);
pub const SIDEBAR_LIST_ID: ComponentId = ComponentId(609);
pub const SIDEBAR_RENAME_ROW_ID: ComponentId = ComponentId(610);
pub const SIDEBAR_RENAME_INPUT_ID: ComponentId = ComponentId(611);
pub const SIDEBAR_RENAME_BTN_ID: ComponentId = ComponentId(612);
pub const SIDEBAR_BRAND_LABEL_ID: ComponentId = ComponentId(613);
pub const SIDEBAR_TOP_ACTIONS_ID: ComponentId = ComponentId(614);
pub const SIDEBAR_SEARCH_TOGGLE_BTN_ID: ComponentId = ComponentId(615);
pub const SIDEBAR_SEARCH_SLOT_ID: ComponentId = ComponentId(616);
pub const SIDEBAR_SECTION_CAPTION_ID: ComponentId = ComponentId(617);
pub const SIDEBAR_SETTINGS_BTN_ID: ComponentId = ComponentId(618);
pub const SIDEBAR_CONTEXT_MENU_ID: ComponentId = ComponentId(619);
pub const SIDEBAR_NEW_ICON_ID: ComponentId = ComponentId(620);
pub const SIDEBAR_SEARCH_ICON_ID: ComponentId = ComponentId(621);
pub const SIDEBAR_SETTINGS_ICON_ID: ComponentId = ComponentId(622);
pub const SIDEBAR_RENAME_CANCEL_BTN_ID: ComponentId = ComponentId(623);
pub const SIDEBAR_CONTEXT_MENUS_ID: ComponentId = ComponentId(626);
pub const SIDEBAR_ROW_BASE: u64 = 10_000;
pub const SIDEBAR_ROW_CONTEXT_MENU_BASE: u64 = 15_000;
pub const SIDEBAR_ROW_DECOR_BASE: u64 = 50_000;

// Summary surface: 700..=999 (+ passive rows 20_000+, decor 60_000+)
pub const SUMMARY_SHELL_ID: ComponentId = ComponentId(700);
pub const SUMMARY_SCROLL_ID: ComponentId = ComponentId(701);
pub const SUMMARY_STACK_ID: ComponentId = ComponentId(702);
pub const SUMMARY_HEADER_ID: ComponentId = ComponentId(703);
pub const SUMMARY_USAGE_CAPTION_ID: ComponentId = ComponentId(704);
pub const SUMMARY_TASKS_HEADER_ID: ComponentId = ComponentId(705);
pub const SUMMARY_TASKS_STACK_ID: ComponentId = ComponentId(706);
pub const SUMMARY_NAV_HEADER_ID: ComponentId = ComponentId(707);
pub const SUMMARY_USAGE_DISC_ID: ComponentId = ComponentId(710);
pub const SUMMARY_USAGE_TRIGGER_ID: ComponentId = ComponentId(711);
pub const SUMMARY_USAGE_TABLE_ID: ComponentId = ComponentId(712);
pub const SUMMARY_TASKS_DISC_ID: ComponentId = ComponentId(720);
pub const SUMMARY_TASKS_TRIGGER_ID: ComponentId = ComponentId(721);
pub const SUMMARY_TASKS_TABLE_ID: ComponentId = ComponentId(722);
pub const SUMMARY_LIST_SCROLL_ID: ComponentId = ComponentId(729);
pub const SUMMARY_LIST_ID: ComponentId = ComponentId(730);
pub const SUMMARY_ACTIONS_ROW_ID: ComponentId = ComponentId(740);
pub const SUMMARY_OPEN_ARTIFACT_BTN_ID: ComponentId = ComponentId(741);
pub const SUMMARY_OPEN_SUBAGENT_BTN_ID: ComponentId = ComponentId(742);
pub const SUMMARY_OPEN_SIDECHAT_BTN_ID: ComponentId = ComponentId(743);
pub const SUMMARY_HOST_SECTIONS_ID: ComponentId = ComponentId(750);
pub const SUMMARY_ROW_BASE: u64 = 20_000;
pub const SUMMARY_KV_BASE: u64 = 21_000;
pub const SUMMARY_ROW_DECOR_BASE: u64 = 60_000;
pub const SUMMARY_TASK_ROW_BASE: u64 = 65_000;

// Workspace Files surface: 1100..=1199 (+ passive tree rows 30_000+, decor 70_000+)
pub const FILES_SHELL_ID: ComponentId = ComponentId(1100);
pub const FILES_ROOT_ID: ComponentId = ComponentId(1101);
pub const FILES_HEADER_ID: ComponentId = ComponentId(1102);
pub const FILES_SEARCH_ID: ComponentId = ComponentId(1103);
pub const FILES_TREE_SCROLL_ID: ComponentId = ComponentId(1104);
pub const FILES_TREE_ID: ComponentId = ComponentId(1105);
pub const FILES_ACTIONS_ROW_ID: ComponentId = ComponentId(1106);
pub const FILES_OPEN_BTN_ID: ComponentId = ComponentId(1107);
pub const FILES_ATTACH_BTN_ID: ComponentId = ComponentId(1108);
pub const FILES_REFRESH_BTN_ID: ComponentId = ComponentId(1109);
pub const FILES_HEADER_ROW_ID: ComponentId = ComponentId(1110);
pub const FILES_REFRESH_ICON_ID: ComponentId = ComponentId(1111);
pub const FILES_CONTEXT_MENU_ID: ComponentId = ComponentId(1112);
pub const FILES_ROW_BASE: u64 = 30_000;
pub const FILES_ROW_DECOR_BASE: u64 = 70_000;

// Workspace Changes surface: 1200..=1299 (+ passive list rows 31_000+)
pub const CHANGES_SHELL_ID: ComponentId = ComponentId(1200);
pub const CHANGES_ROOT_ID: ComponentId = ComponentId(1201);
pub const CHANGES_HEADER_ID: ComponentId = ComponentId(1202);
pub const CHANGES_MODE_SEG_ID: ComponentId = ComponentId(1203);
pub const CHANGES_FILES_SCROLL_ID: ComponentId = ComponentId(1204);
pub const CHANGES_FILES_LIST_ID: ComponentId = ComponentId(1205);
pub const CHANGES_DIFF_SCROLL_ID: ComponentId = ComponentId(1206);
pub const CHANGES_DIFF_RICH_ID: ComponentId = ComponentId(1207);
pub const CHANGES_COMMIT_ROW_ID: ComponentId = ComponentId(1208);
pub const CHANGES_QUOTE_BTN_ID: ComponentId = ComponentId(1209);
pub const CHANGES_COMMIT_INPUT_ID: ComponentId = ComponentId(1210);
pub const CHANGES_COMMIT_BTN_ID: ComponentId = ComponentId(1211);
pub const CHANGES_HEADER_ROW_ID: ComponentId = ComponentId(1212);
pub const CHANGES_QUOTE_ICON_ID: ComponentId = ComponentId(1213);
pub const CHANGES_DIFF_PANEL_ID: ComponentId = ComponentId(1214);
pub const CHANGES_ROW_BASE: u64 = 31_000;
pub const CHANGES_ROW_DECOR_BASE: u64 = 80_000;

// Artifact Viewer surface: 1300..=1499
pub const ARTIFACT_SHELL_ID: ComponentId = ComponentId(1300);
pub const ARTIFACT_ROOT_ID: ComponentId = ComponentId(1301);
pub const ARTIFACT_HEADER_ID: ComponentId = ComponentId(1302);
pub const ARTIFACT_ACTIONS_ROW_ID: ComponentId = ComponentId(1303);
pub const ARTIFACT_QUOTE_BTN_ID: ComponentId = ComponentId(1304);
pub const ARTIFACT_ATTACH_BTN_ID: ComponentId = ComponentId(1305);
pub const ARTIFACT_RELOAD_BTN_ID: ComponentId = ComponentId(1306);
pub const ARTIFACT_BODY_SLOT_ID: ComponentId = ComponentId(1307);
pub const ARTIFACT_BODY_SCROLL_ID: ComponentId = ComponentId(1308);
pub const ARTIFACT_HEADER_ROW_ID: ComponentId = ComponentId(1309);
pub const ARTIFACT_CODE_EDITOR_ID: ComponentId = ComponentId(1310);
pub const ARTIFACT_MARKDOWN_RICH_ID: ComponentId = ComponentId(1311);
pub const ARTIFACT_IMAGE_ID: ComponentId = ComponentId(1312);
pub const ARTIFACT_DIAG_TEXT_ID: ComponentId = ComponentId(1313);
pub const ARTIFACT_SEP_ID: ComponentId = ComponentId(1314);
pub const ARTIFACT_QUOTE_ICON_ID: ComponentId = ComponentId(1315);
pub const ARTIFACT_ATTACH_ICON_ID: ComponentId = ComponentId(1316);
pub const ARTIFACT_RELOAD_ICON_ID: ComponentId = ComponentId(1317);
pub const ARTIFACT_CUSTOM_SLOT_ID: ComponentId = ComponentId(1400);

// Tasks & Queue surface: 1500..=1599 (+ passive rows 32_000+)
pub const TASKS_SHELL_ID: ComponentId = ComponentId(1500);
pub const TASKS_ROOT_ID: ComponentId = ComponentId(1501);
pub const TASKS_HEADER_ID: ComponentId = ComponentId(1502);
pub const TASKS_SCROLL_ID: ComponentId = ComponentId(1503);
pub const TASKS_STACK_ID: ComponentId = ComponentId(1504);
pub const TASKS_ERROR_ID: ComponentId = ComponentId(1505);
pub const TASKS_QUEUE_SECTION_ID: ComponentId = ComponentId(1506);
pub const TASKS_QUEUE_EDIT_ROW_ID: ComponentId = ComponentId(1507);
pub const TASKS_QUEUE_EDIT_INPUT_ID: ComponentId = ComponentId(1508);
pub const TASKS_QUEUE_APPLY_BTN_ID: ComponentId = ComponentId(1509);
pub const TASKS_QUEUE_REMOVE_BTN_ID: ComponentId = ComponentId(1510);
pub const TASKS_JOBS_SECTION_ID: ComponentId = ComponentId(1511);
pub const TASKS_QUEUE_ROW_BASE: u64 = 32_000;
pub const TASKS_TASK_ROW_BASE: u64 = 32_500;
pub const TASKS_TASK_OPEN_BTN_BASE: u64 = 33_000;
pub const TASKS_TASK_ICON_BASE: u64 = 90_000;
pub const TASKS_TASK_CONTENT_BASE: u64 = 91_000;

// ChildRun surface: 1600..=1699 (+ passive rows 34_000+)
pub const CHILD_RUN_SHELL_ID: ComponentId = ComponentId(1600);
pub const CHILD_RUN_ROOT_ID: ComponentId = ComponentId(1601);
pub const CHILD_RUN_HEADER_ID: ComponentId = ComponentId(1602);
pub const CHILD_RUN_ACTIONS_ROW_ID: ComponentId = ComponentId(1603);
pub const CHILD_RUN_EARLIER_BTN_ID: ComponentId = ComponentId(1604);
pub const CHILD_RUN_REFRESH_BTN_ID: ComponentId = ComponentId(1605);
pub const CHILD_RUN_READER_ID: ComponentId = ComponentId(1606);
pub const CHILD_RUN_STATUS_ID: ComponentId = ComponentId(1607);
pub const CHILD_RUN_HEADER_ROW_ID: ComponentId = ComponentId(1608);
pub const CHILD_RUN_REFRESH_ICON_ID: ComponentId = ComponentId(1609);
pub const CHILD_RUN_ROW_BASE: u64 = 34_000;

// SideChat surface: 1700..=1799
pub const SIDECHAT_SHELL_ID: ComponentId = ComponentId(1700);
pub const SIDECHAT_ROOT_ID: ComponentId = ComponentId(1701);
pub const SIDECHAT_HEADER_ID: ComponentId = ComponentId(1702);
pub const SIDECHAT_ACTIONS_ROW_ID: ComponentId = ComponentId(1703);
pub const SIDECHAT_ACCESS_BADGE_ID: ComponentId = ComponentId(1704);
pub const SIDECHAT_CREATE_BTN_ID: ComponentId = ComponentId(1705);
pub const SIDECHAT_CLOSE_BTN_ID: ComponentId = ComponentId(1706);
pub const SIDECHAT_SCROLL_ID: ComponentId = ComponentId(1707);
pub const SIDECHAT_RICH_ID: ComponentId = ComponentId(1708);
pub const SIDECHAT_COMPOSER_ROW_ID: ComponentId = ComponentId(1709);
pub const SIDECHAT_EDITOR_ID: ComponentId = ComponentId(1710);
pub const SIDECHAT_SEND_BTN_ID: ComponentId = ComponentId(1711);
pub const SIDECHAT_ERROR_ID: ComponentId = ComponentId(1712);
pub const SIDECHAT_HEADER_ROW_ID: ComponentId = ComponentId(1713);
pub const SIDECHAT_COMPOSER_PANEL_ID: ComponentId = ComponentId(1714);

// Harness Settings surface: 1800..=1899 (+ dynamic controls 38_000+)
pub const SETTINGS_SHELL_ID: ComponentId = ComponentId(1800);
pub const SETTINGS_ROOT_ID: ComponentId = ComponentId(1801);
pub const SETTINGS_HEADER_ID: ComponentId = ComponentId(1802);
pub const SETTINGS_BANNER_ID: ComponentId = ComponentId(1803);
pub const SETTINGS_ACTIONS_ROW_ID: ComponentId = ComponentId(1804);
pub const SETTINGS_REFRESH_BTN_ID: ComponentId = ComponentId(1805);
pub const SETTINGS_APPLY_BTN_ID: ComponentId = ComponentId(1806);
pub const SETTINGS_FIELDS_SCROLL_ID: ComponentId = ComponentId(1807);
pub const SETTINGS_FIELDS_STACK_ID: ComponentId = ComponentId(1808);
pub const SETTINGS_HEADER_ROW_ID: ComponentId = ComponentId(1809);
pub const SETTINGS_REFRESH_ICON_ID: ComponentId = ComponentId(1810);
pub const SETTINGS_FIELD_BASE: u64 = 38_000;
pub const SETTINGS_CONTROL_BASE: u64 = 39_000;

/// Replace all dynamic children of a mutable `StackContent` container while keeping `ComponentHost` IDs clean.
pub fn replace_stack_children(
    host: &mut tekes_ui_controls::composition::ContentHost,
    parent: ComponentId,
    new_children: Vec<tekes_ui_controls::composition::Content>,
) {
    let existing_ids: Vec<ComponentId> = host
        .get(parent)
        .and_then(|node| node.component::<tekes_ui_controls::composition::StackContent>())
        .map(|stack| stack.children().iter().map(|c| c.id()).collect())
        .unwrap_or_default();
    for id in existing_ids {
        let _ = host.remove(id);
    }
    for child in new_children {
        let _ = host.insert(parent, child);
    }
}
