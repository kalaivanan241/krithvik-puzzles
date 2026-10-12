CREATE TABLE IF NOT EXISTS animals (
    key TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    sound TEXT NOT NULL CHECK (length(trim(sound)) > 0),
    sort_order INTEGER NOT NULL
);

-- Seed missing records only: restarting must not overwrite database edits.
INSERT INTO animals (key, name, sound, sort_order) VALUES
    ('cat', 'Cat', 'Meow! Meow!', 1),
    ('dog', 'Dog', 'Woof! Woof!', 2),
    ('cow', 'Cow', 'Moo! Moooo!', 3),
    ('duck', 'Duck', 'Quack! Quack!', 4),
    ('lion', 'Lion', 'Roar! Roooar!', 5),
    ('frog', 'Frog', 'Ribbit! Ribbit!', 6),
    ('pig', 'Pig', 'Oink! Oink!', 7),
    ('sheep', 'Sheep', 'Baa! Baaa!', 8)
ON CONFLICT(key) DO NOTHING;
