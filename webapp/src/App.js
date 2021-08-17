import { BrowserRouter, Route } from "react-router-dom";
import Container from 'react-bootstrap/Container';
import Nav from 'react-bootstrap/Nav';
import Navbar from 'react-bootstrap/Navbar';
import SearchPage from './SearchPage/Search';
import VisitPage from './VisitPage/VisitPage';

function App() {
	return (
		<BrowserRouter>
			<Navigation />
			<Route path="/" exact component={SearchPage} />
			<Route path="/patients/:patientId" exact component={VisitPage} />
		</BrowserRouter>
	);
}

function Navigation() {
	return (
		<Navbar>
			<Container>
				<Navbar.Brand href="#home">Clinic</Navbar.Brand>
				<Navbar.Collapse className="justify-content-end">
					<Nav>
						<Nav.Link href="/">Search</Nav.Link>
						<Nav.Link eventKey={2} href="/backup">
							Backup
						</Nav.Link>
					</Nav>
				</Navbar.Collapse>
			</Container>
		</Navbar>
	)
}

export default App;
