import Container from 'react-bootstrap/Container';
import Row from 'react-bootstrap/Row';
import FormControl from 'react-bootstrap/FormControl';
import Table from 'react-bootstrap/Table';
import Button from 'react-bootstrap/Button';

function SearchPage() {
	return (
		<Container>
			<Row style={{ marginBottom: 20 }}>
				<FormControl type="input" placeholder="Search" autoFocus={true} />
			</Row>
			<Row>
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
						<tr>
							<td colSpan={5} align="center">No patient found</td>
						</tr>
						<tr>
							<td>Lorem</td>
							<td>Ipsum</td>
							<td>June, 18 20201 (24 yo)</td>
							<td>Jl kutilangraya</td>
							<td>August, 17 2021 (yesterday)</td>
						</tr>
						<tr>
							<td>Lorem</td>
							<td>Ipsum</td>
							<td>June, 18 20201 (24 yo)</td>
							<td>Jl kutilangraya</td>
							<td>August, 17 2021 (yesterday)</td>
						</tr>
						<tr>
							<td>Lorem</td>
							<td>Ipsum</td>
							<td>June, 18 20201 (24 yo)</td>
							<td>Jl kutilangraya</td>
							<td>August, 17 2021 (yesterday)</td>
						</tr>
						<tr>
							<td>Lorem</td>
							<td>Ipsum</td>
							<td>June, 18 20201 (24 yo)</td>
							<td>Jl kutilangraya</td>
							<td>August, 17 2021 (yesterday)</td>
						</tr>
					</tbody>
				</Table>
			</Row>
			<Row>
				<Button variant="link">Load more...</Button>
			</Row>
		</Container>
	);
}

export default SearchPage;
