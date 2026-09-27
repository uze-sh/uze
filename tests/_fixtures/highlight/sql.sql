CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

SELECT name FROM users WHERE id > 10 ORDER BY name;
