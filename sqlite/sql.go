package sqlite

const tableInitSQL = `
create table if not exists patients (
	id text primary key,
	name text,
	head_family_name text,
	birth_date text,
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
	timestamp integer,
	diagnosis text,
	is_deleted bool
);
create index if not exists visits_patient_id_idx on visits(patient_id, timestamp);
create index if not exists visits_id_idx on visits(id);
`

const getPatientSQL = "SELECT * FROM patients WHERE id = ?"

const insertPatientSQL = "INSERT INTO patients (id, name, head_family_name, birth_date, address, last_visit) VALUES (?,?,?,?,?,?)"

const insertIndexSQL = "INSERT INTO patient_indices (keyword, last_visit, patient_id) VALUES (?, ?, ?)"

const updatePatientSQL = "UPDATE patients SET name=?, head_family_name=?, birth_date=?, address=? WHERE id=?"

const deleteIndexSQL = "DELETE FROM patient_indices WHERE patient_id = ?"
