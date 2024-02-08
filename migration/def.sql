CREATE TABLE commands (
    id VARBINARY(16) PRIMARY KEY NOT NULL,
    command_type VARCHAR(20) NOT NULL,
    discussion_link VARCHAR(60) NOT NULL
);

CREATE TABLE command_target_namespaces (
    command_id VARBINARY(16) NOT NULL,
    namespace INTEGER NOT NULL,
    CONSTRAINT command_target_namespace_command
        FOREIGN KEY (command_id) REFERENCES commands (id),
    PRIMARY KEY (command_id, namespace)
);

CREATE TABLE command_from_categories (
    command_id VARBINARY(16) NOT NULL,
    category VARCHAR(60) NOT NULL,
    CONSTRAINT command_from_categories_command
        FOREIGN KEY (command_id) REFERENCES commands (id),
    PRIMARY KEY (command_id, category)
);

CREATE TABLE command_to_categories (
    command_id VARBINARY(16) NOT NULL,
    category VARCHAR(60) NOT NULL,
    CONSTRAINT command_to_categories_command
        FOREIGN KEY (command_id) REFERENCES commands (id),
    PRIMARY KEY (command_id, category)
);

CREATE TABLE operations (
    id VARBINARY(16) PRIMARY KEY NOT NULL,
    command_id VARBINARY(16) NOT NULL,
    page_id INTEGER NOT NULL,
    rev_id INTEGER NOT NULL,
    CONSTRAINT operation_command
        FOREIGN KEY (command_id) REFERENCES commands (id)
);
