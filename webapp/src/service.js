const baseURL = 'http://localhost:5000/api/v1'

export async function searchPatients(q, cursor) {
	return fetch(baseURL + '/search/?' + new URLSearchParams({ q, cursor }))
		.then(res => res.json())
		.catch(_ => ({ 'errorMessage': 'Something went wrong' }))
}

export async function getPatient(id) {
	return fetch(`${baseURL}/patients/${id}/`).
		then(res => res.json()).
		catch(_ => ({ 'errorMessage': 'Something went wrong' }))
}

export async function postPatient(patient) {
	return fetch(`${baseURL}/patients/`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(patient),
		mode: 'cors',
		crossDomain: true,
	}).
		then(res => res.json()).
		catch(_ => ({ 'errorMessage': 'Something went wrong' }))
}

export async function deletePatient(patient) {
	return fetch(`${baseURL}/patients/${patient.id}/`, {
		method: 'DELETE',
		mode: 'cors',
		crossDomain: true,
	}).
		then(res => res.json()).
		catch(_ => ({ 'errorMessage': 'Something went wrong' }))
}

export async function putPatient(patient) {
	return fetch(`${baseURL}/patients/${patient['id']}/`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(patient),
		mode: 'cors',
		crossDomain: true,
	}).
		then(res => res.json()).
		catch(_ => ({ 'errorMessage': 'Something went wrong' }))
}

export async function insertVisit(patientVisit) {
	return fetch(`${baseURL}/patients/${patientVisit['patientId']}/visits/`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(patientVisit),
		mode: 'cors',
		crossDomain: true,
	}).
		then(res => res.json()).
		catch(_ => ({ 'errorMessage': 'Something went wrong' }))
}

export async function getPatientVisits(patientId, cursor) {
	return fetch(`${baseURL}/patients/${patientId}/visits/?` + new URLSearchParams({ cursor })).
		then(res => res.json()).
		catch(_ => ({ 'errorMessage': 'Something went wrong' }))
}

