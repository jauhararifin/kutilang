#!/bin/bash

# ====================================== valid patient =========================================

echo Insert a valid patient

curl -L -X POST "http://localhost:5000/api/v1/patients/" --header 'Content-Type: application/json'  --data-raw '{
    "name": "Jauhar",
    "headFamilyName": "Arifin",
    "birthDate": "1997-06-18",
    "address": "jl kutilang raya"
}' -o /tmp/output1 2> /dev/null

echo curl result is:
cat /tmp/output1

PATIENT1_ID=`cat /tmp/output1 | jq -r .result.id`
echo inserted patient: $PATIENT1_ID

echo patients table:
sqlite3 ~/.config/kutilang/db.sqlite3 "SELECT * FROM patients"

echo index table:
sqlite3 ~/.config/kutilang/db.sqlite3 "SELECT * FROM patient_indices"

# ====================================== get patient ===============================================

echo
echo Get patient
curl -L "http://localhost:5000/api/v1/patients/$PATIENT1_ID" 
curl -L "http://localhost:5000/api/v1/patients/somerandomid" 

# ====================================== invalid birth date =========================================

echo
echo Insert invalid birth date
curl -L -X POST "http://localhost:5000/api/v1/patients/" --header 'Content-Type: application/json'  --data-raw '{
    "name": "Jauhar",
    "headFamilyName": "Arifin",
    "birthDate": "1900-02-29",
    "address": "jl kutilang raya"
}'

# ====================================== update patient =========================================

echo
echo Update a patient
curl -L -X PUT "http://localhost:5000/api/v1/patients/$PATIENT1_ID" --header 'Content-Type: application/json'  --data-raw '{
	 "name": "Lorem",
	 "headFamilyName": "Ipsum",
	 "birthDate": "1997-03-29",
	 "address": "jl kutilang raya"
}'

echo patients table:
sqlite3 ~/.config/kutilang/db.sqlite3 "SELECT * FROM patients"

echo index table:
sqlite3 ~/.config/kutilang/db.sqlite3 "SELECT * FROM patient_indices"

# ====================================== delete patient =========================================

echo
echo Delete a patient
curl -L -X DELETE "http://localhost:5000/api/v1/patients/$PATIENT1_ID" 

echo patients table:
sqlite3 ~/.config/kutilang/db.sqlite3 "SELECT * FROM patients"

echo index table:
sqlite3 ~/.config/kutilang/db.sqlite3 "SELECT * FROM patient_indices"

# ====================================== insert patient fixtures =========================================

./scripts/fixtures.sh

# ====================================== search patient ==========================================

# TODO (jauhararifin): add search patient

# ====================================== insert visit ============================================

echo
echo inserting entries for $PATIENT1_ID
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
curl -L -X POST "http://localhost:5000/api/v1/patients/$PATIENT1_ID/visits/" --header 'Content-Type: application/json'  --data-raw '{"diagnosis": "Lorem <b>ipsum</b> dos color sit amet."}'
