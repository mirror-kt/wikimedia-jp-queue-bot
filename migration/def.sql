CREATE TABLE commands (
    id VARBINARY(16) PRIMARY KEY NOT NULL,
    command_type VARCHAR(20) NOT NULL,
    params JSON NOT NULL
);

CREATE TABLE operations (
    id VARBINARY(16) PRIMARY KEY NOT NULL,
    command_id VARBINARY(16) NOT NULL,
    operation_type VARCHAR(20) NOT NULL,
    page_id INTEGER NOT NULL,
    rev_id INTEGER NOT NULL,
    CONSTRAINT operation_command FOREIGN KEY (command_id) REFERENCES commands (id)
);