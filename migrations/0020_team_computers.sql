-- REQ-031: the space's computers are either private (per-bot) or team (shared within the
-- trust boundary). A team computer is one row for the whole space, however many bots use it.
--
-- `private` was the default before this migration and every existing row meant per-bot
-- isolation; it now says so explicitly. At most one team computer exists per space.

UPDATE computers SET private = true WHERE NOT private;

CREATE UNIQUE INDEX computers_team_per_space ON computers (space_id) WHERE NOT private;
