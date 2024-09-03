DROP TABLE IF EXISTS `devices`;
CREATE TABLE `devices` (
  `id` integer PRIMARY KEY AUTOINCREMENT,
  `device_id` varchar UNIQUE NOT NULL,
  `version` varchar NOT NULL,
  `arch` varchar NOT NULL,
  `os` varchar NOT NULL,
  `client_address` varchar NOT NULL,
  `login_time` bigint ,
  `meta` text ,
  `created_at` bigint  DEFAULT (strftime('%s', 'now')),
  `updated_at` bigint  DEFAULT (strftime('%s', 'now'))
);

DROP TABLE IF EXISTS `node_status`;
CREATE TABLE `node_status` (
  `id` integer PRIMARY KEY AUTOINCREMENT,
  `node_id` varchar UNIQUE NOT NULL,
  `device_id` varchar NOT NULL,
  `subdomain` varchar UNIQUE DEFAULT "",
  `version` varchar NOT NULL,
  `arch` varchar NOT NULL,
  `os` varchar NOT NULL,
  `client_address` varchar NOT NULL,
  `login_time` bigint ,
  `last_active_time` bigint ,
  `last_avail_time` bigint ,
  `run_id` varchar DEFAULT "",
  `meta` text ,
  `node_version` varchar DEFAULT "",
  `chat_model` varchar DEFAULT "",
  `embedding_model` varchar DEFAULT "",
  `status` varchar DEFAULT 'unknown',
  `created_at` bigint DEFAULT (strftime('%s', 'now')),
  `updated_at` bigint DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX idx_status ON node_status (status);
CREATE INDEX idx_login_time ON node_status (login_time);
CREATE INDEX idx_last_active_time ON node_status (last_active_time);
CREATE INDEX idx_last_avail_time ON node_status (last_avail_time);
