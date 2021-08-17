import { useState } from 'react';
import Container from 'react-bootstrap/Container';
import Table from 'react-bootstrap/Table';
import Card from 'react-bootstrap/Card';
import Button from 'react-bootstrap/Button';
import Row from 'react-bootstrap/Row';
import { BsPencil, BsTrash } from "react-icons/bs";
import { Editor } from "react-draft-wysiwyg";

import "react-draft-wysiwyg/dist/react-draft-wysiwyg.css";

function VisitPage() {
	return <Container>
		<h5 style={{ textAlign: "center", marginBottom: 30, }}>Profile</h5>
		<Profile />

		<h5 style={{ textAlign: "center", marginBottom: 30, marginTop: 20 }}>New Entry</h5>
		<NewVisit />

		<h5 style={{ textAlign: "center", marginBottom: 30, marginTop: 20 }}>History</h5>
		<Visit />
		<Visit />

		<Row style={{marginBottom: 20}}>
			<Button variant="link">Load more...</Button>
		</Row>
	</Container>
}

function Profile() {
	return <Table striped bordered hover>
		<tbody>
			<tr>
				<th style={{ width: 200 }}>Name</th>
				<td>Lorem</td>
			</tr>
			<tr>
				<th>Family Name</th>
				<td>Ipsum</td>
			</tr>
			<tr>
				<th>Birthdate</th>
				<td>June, 18 1997 (24 years old)</td>
			</tr>
			<tr>
				<th>Address</th>
				<td>Perum kutilangsari</td>
			</tr>
			<tr>
				<th>Last Visit</th>
				<td>August, 17 2021 (yesterday)</td>
			</tr>
		</tbody>
	</Table>;
}

function NewVisit() {
	const [editorState, setEditorState] = useState();
	return (
		<Card style={{ marginBottom: 20 }}>
			<Card.Header>August, 17 2021 (yesterday)</Card.Header>
			<Card.Body style={{ padding: 0 }}>
				<Editor
					editorState={editorState}
					onEditorStateChange={setEditorState}
					toolbar={{ options: ['inline', 'blockType', 'fontSize', 'list', 'textAlign', 'colorPicker', 'link', 'emoji', 'remove', 'history'], }}
					toolbarStyle={{ border: 0, padding: 0 }}
					editorStyle={{ padding: 10 }}
				/>
			</Card.Body>
			<Card.Footer style={{ textAlign: "right" }}>
				<Button variant="primary">Submit</Button>
			</Card.Footer>
		</Card>
	)
}

function Visit() {
	return (
		<Card style={{ marginBottom: 20 }}>
			<Card.Header>August, 17 2021 (yesterday)</Card.Header>
			<Card.Body>
				<blockquote className="blockquote mb-0">
					<p>
						{' '}
						Lorem ipsum dolor sit amet, consectetur adipiscing elit. Integer posuere
						erat a ante.{' '}
					</p>
					<footer className="blockquote-footer">
						Someone famous in <cite title="Source Title">Source Title</cite>
					</footer>
				</blockquote>
			</Card.Body>
			<Card.Footer style={{ textAlign: "right" }}>
				<Button variant="link"><BsPencil /></Button>
				<Button variant="link"><BsTrash /></Button>
			</Card.Footer>
		</Card>
	)
}

export default VisitPage;
