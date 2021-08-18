package sqlite

const tableInitSQL = `
create table if not exists patients (
	id text primary key,
	name text,
	head_family_name text,
	birthdate integer,
	address text,
	last_visit integer
);
create index if not exists patients_id_idx on patients(id, last_visit);

create table if not exists patient_indices (
	keyword text,
	last_visit integer,
	patient_id text
);
create index if not exists patient_indices_idx on patient_indices(keyword, last_visit, patient_id);

create table if not exists visits (
	id text primary key,
	patient_id text,
	date integer,
	diagnosis text,
	is_deleted bool
);
create index if not exists visits_patient_id_idx on visits(patient_id, date);
create index if not exists visits_id_idx on visits(id);
`

const insertPatientSQL = "INSERT INTO patients (id, name, head_family_name, birthdate, address, last_visit) VALUES (?,?,?,?,?,?)"

const insertIndexSQL = "INSERT INTO patient_indices (keyword, last_visit, patient_id) VALUES (?, ?, ?)"
