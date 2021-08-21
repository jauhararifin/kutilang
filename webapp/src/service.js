import axios from 'axios'

const baseUrl = 'http://localhost:5000/api/v1'

export async function searchPatients(q, cursor) {
	const res = await axios.get(baseUrl + '/search/', {
		params: {q, cursor},
	})
	return res.data
}
