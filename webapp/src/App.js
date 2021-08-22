import Container from 'react-bootstrap/Container';
import Nav from 'react-bootstrap/Nav';
import Navbar from 'react-bootstrap/Navbar';
import { BrowserRouter, Route } from "react-router-dom";
import SearchPage from './SearchPage/Search';
import VisitPage from './VisitPage/VisitPage';

function App() {
	return (
		<BrowserRouter>
			<Navigation />
			<Route path="/" exact component={SearchPage} />
			<Route path="/patients/" exact component={VisitPage} />
			<Route path="/patients/:patientId" exact component={VisitPage} />
		</BrowserRouter>
	);
}

function Navigation() {
	return (
		<Navbar>
			<Container>
				<Navbar.Brand href="/">Clinic</Navbar.Brand>
				<Navbar.Collapse className="justify-content-end">
					<Nav>
						<Nav.Link href="/">Search</Nav.Link>
						<Nav.Link href="/patients">New</Nav.Link>
					</Nav>
				</Navbar.Collapse>
			</Container>
		</Navbar>
	)
}

export default App;
