import moment from 'moment';
import Noty from 'noty';
import { useEffect, useState } from 'react';
import Container from 'react-bootstrap/Container';
import FormControl from 'react-bootstrap/FormControl';
import Row from 'react-bootstrap/Row';
import Spinner from 'react-bootstrap/Spinner';
import Table from 'react-bootstrap/Table';
import { useHistory, useLocation } from "react-router-dom";
import { searchPatients } from '../service';

function SearchPage() {
	const search = useLocation().search;
	const initialQuery = new URLSearchParams(search).get('q');

	const [query, setQuery] = useState(initialQuery);
	const [isLoading, setIsLoading] = useState(!!initialQuery);
	const [patients, setPatients] = useState([])
	const [nextCursor, setNextCursor] = useState('')
	const hasNext = !!nextCursor

	useEffect(async () => { 
		if (!query) return
		setIsLoading(true)
		const result = await searchPatients(query, '')
		if (result['errorMessage']) {
			new Noty({text: result['errorMessage'], layout: 'topCenter', type:'error', theme: 'bootstrap-v4'}).show();
		} else {
			setPatients(result['result']['results'])
			setNextCursor(result['result']['nextCursor'])
		}
		setIsLoading(false)
	}, [query])

	const loadNext = async () => {
		setIsLoading(true)
		const result = await searchPatients(query, nextCursor)
		if (result['errorMessage']) {
			new Noty({text: result['errorMessage'], layout: 'topCenter', type:'error', theme: 'bootstrap-v4'}).show();
		} else {
			setPatients([...patients, ...result['result']['results']])
			setNextCursor(result['result']['nextCursor'])
		}
		setIsLoading(false)
	}

	return <SearchView
		query={query}
		setQuery={setQuery}
		isLoading={isLoading}
		patients={patients}
		hasNext={hasNext}
		loadNext={loadNext} />
}

function SearchView({query, setQuery, isLoading, patients, hasNext, loadNext}) {
	const [q, setQ] = useState(query || '')
	return (
		<Container>
			<form onSubmit={event => {
				event.preventDefault();
				setQuery(q)
			}}>
				<Row style={{ marginBottom: 20 }}>
					<FormControl
						type="search"
						placeholder="Search"
						autoFocus={true}
						value={q}
						onChange={ev=>setQ(ev.target.value)}
					/>
				</Row>
			</form>
			<Row>
				<SearchTableResultView isLoading={isLoading} patients={patients} hasNext={hasNext} loadNext={loadNext} />
			</Row>
		</Container>
	)
}

function SearchTableResultView({isLoading, patients, hasNext, loadNext}) {
	const history = useHistory();
	return (
		<Table striped bordered hover>
			<thead>
				<tr>
					<th>Name</th>
					<th>Family Name</th>
					<th>Date Birth</th>
					<th>Address</th>
					<th>Last Visit</th>
				</tr>
			</thead>
			<tbody>
				{ !isLoading && patients.length == 0 && <tr>
					<td colSpan={5} align="center">No patient found</td>
				</tr> }

				{ patients.map(patient => (
					<tr key={patient.id} style={{cursor: 'pointer'}} onClick={()=>history.push(`/patients/${patient.id}`)}>
						<td>{patient['name']}</td>
						<td>{patient['headFamilyName']}</td>
						<td>{moment(patient['birthDate']).format('LL')} ({moment().diff(patient['birthDate'], 'years')} years old)</td>
						<td>{patient['address']}</td>
						<td>{moment(patient['lastVisit']).format('LL')} ({moment(patient['lastVisit']).fromNow()})</td>
					</tr>
				))}

				{ isLoading && <tr style={{textAlign:'center'}}>
					<td colSpan={5}><Spinner animation="border" role="status"><span className="visually-hidden">Loading...</span></Spinner></td>
				</tr>}

				{ !isLoading && hasNext && <tr onClick={loadNext} style={{cursor: 'pointer'}}>
					<td colSpan={5} align="center"><a href="#">Load more</a></td>
				</tr> }
			</tbody>
		</Table>
	)
}

export default SearchPage;
