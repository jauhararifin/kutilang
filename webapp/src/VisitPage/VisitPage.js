import { useFormik } from 'formik';
import moment from 'moment';
import 'moment/locale/id';
import Noty from 'noty';
import { useEffect, useState } from 'react';
import Button from 'react-bootstrap/Button';
import Card from 'react-bootstrap/Card';
import Container from 'react-bootstrap/Container';
import Form from 'react-bootstrap/Form';
import InputGroup from 'react-bootstrap/InputGroup';
import Row from 'react-bootstrap/Row';
import Spinner from 'react-bootstrap/Spinner';
import Table from 'react-bootstrap/Table';
import { Editor } from "react-draft-wysiwyg";
import "react-draft-wysiwyg/dist/react-draft-wysiwyg.css";
import { BsPencil, BsTrash } from "react-icons/bs";
import { useHistory, useParams } from 'react-router-dom';
import * as Yup from 'yup';
import { deletePatient, getPatient, postPatient, putPatient } from '../service';

const baseURL = "http://localhost:5000"

function showSuccessPopup(text) {
	new Noty({ text: text, layout: 'topCenter', type: 'success', theme: 'bootstrap-v4' }).show();
}

function showErrorPopup(text) {
	new Noty({ text: text, layout: 'topCenter', type: 'error', theme: 'bootstrap-v4' }).show();
}

function VisitPage() {
	const history = useHistory();
	const { patientId } = useParams()
	const isNewPatientPage = !patientId
	const [isLoading, setIsLoading] = useState(false)

	const [patient, setPatient] = useState(null)
	useEffect(async () => {
		if (!patientId) return
		setIsLoading(true)
		const result = await getPatient(patientId)
		if (result['errorMessage']) {
			showErrorPopup(result['errorMessage'])
		} else {
			setPatient(result['result'])
		}
		setIsLoading(false)
	}, [patientId])

	const [isEditingProfile, setIsEditingProfile] = useState(false)

	const createProfile = async (patient) => {
		setIsLoading(true)
		const resp = await postPatient(patient)
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Patient added')
			history.replace(`/patients/${resp['result']['id']}`)
		}
		setIsLoading(false)
	}

	const updateProfile = async (patient) => {
		setIsLoading(true)
		const resp = await putPatient(patient)
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Patient updated')
			setIsEditingProfile(false)
			setPatient(patient)
		}
		setIsLoading(false)
	}

	const editProfile = () => { setIsEditingProfile(true) }

	const deleteProfile = async () => {
		if (!window.confirm('Are you sure you want to delete this patient?'))
			return

		const resp = await deletePatient(patient)
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Patient deleted')
			history.replace('/')
		}
	}

	return <Container>
		{isLoading && <div style={{ textAlign: 'center' }}>
			<Spinner animation="border" role="status"><span className="visually-hidden">Loading...</span></Spinner>
		</div>}

		{isNewPatientPage && <>
			<h5 style={{ textAlign: "center", marginBottom: 30, }}>New Profile</h5>
			<ProfileEditor patient={patient} onSubmit={createProfile} />
		</>}

		{(!isNewPatientPage && isEditingProfile) && <>
			<h5 style={{ textAlign: "center", marginBottom: 30, }}>Update Profile</h5>
			<ProfileEditor patient={patient} onSubmit={updateProfile} />
		</>}

		{!isNewPatientPage && !!patient && !isEditingProfile && <>
			<h5 style={{ textAlign: "center", marginBottom: 30, }}>Profile</h5>
			<Profile patient={patient} onEdit={editProfile} onDelete={deleteProfile} />
		</>}


		{!isNewPatientPage && <>
			<h5 style={{ textAlign: "center", marginBottom: 30, marginTop: 20 }}>New Entry</h5>
			<NewVisit />

			<h5 style={{ textAlign: "center", marginBottom: 30, marginTop: 20 }}>History</h5>
			<Visit />
			<Visit />

			<Row style={{ marginBottom: 20 }}>
				<Button variant="link">Load more...</Button>
			</Row>
		</>}
	</Container>
}

function Profile({ patient, onEdit, onDelete }) {
	return <Table striped bordered hover>
		<tbody>
			<tr>
				<th style={{ width: 200 }}>Name</th>
				<td>{patient.name}</td>
			</tr>
			<tr>
				<th>Family Name</th>
				<td>{patient.headFamilyName}</td>
			</tr>
			<tr>
				<th>Birthdate</th>
				<td>{moment(patient['birthDate']).format('LL')} ({moment().diff(patient['birthDate'], 'years')} years old)</td>
			</tr>
			<tr>
				<th>Address</th>
				<td>{patient['address']}</td>
			</tr>
			<tr>
				<th>Last Visit</th>
				<td>{moment(patient['lastVisit']).format('LL')} ({moment(patient['lastVisit']).fromNow()})</td>
			</tr>
			<tr>
				<td colSpan={2} style={{ textAlign: "right" }}>
					<Button variant="secondary" onClick={onEdit}>Edit</Button>{' '}
					<Button variant="danger" onClick={onDelete}>Delete</Button>
				</td>
			</tr>
		</tbody>
	</Table>
}

function ProfileEditor({ patient, onSubmit }) {
	patient = patient || {}
	const lastVisit = patient['lastVisit'] ? moment(patient['lastVisit']) : moment()
	const formik = useFormik({
		initialValues: {
			name: patient['name'] || '',
			headFamilyName: patient['headFamilyName'] || '',
			birthDate: patient['birthDate'] || '',
			address: patient['address'] || '',
		},
		validationSchema: Yup.object({
			name: Yup.string().max(255, 'Maximum at 255 character').required('Required'),
			headFamilyName: Yup.string().max(255, 'Maximum at 255 character'),
			birthDate: Yup.date().required().max(new Date(), 'Invalid date'),
			address: Yup.string().max(4096).required('Required'),
		}),
		onSubmit: onSubmit,
	});

	return <Form onSubmit={formik.handleSubmit}>
		<Table striped bordered hover>
			<tbody>
				<tr>
					<th style={{ width: 200 }}>Name</th>
					<td>
						<InputGroup>
							<Form.Control type="input" {...formik.getFieldProps('name')} isInvalid={formik.touched.name && formik.errors.name} />
							<Form.Control.Feedback type="invalid">
								{formik.touched.name && formik.errors.name}
							</Form.Control.Feedback>
						</InputGroup>
					</td>
				</tr>
				<tr>
					<th>Family Name</th>
					<td>
						<InputGroup>
							<Form.Control type="input" {...formik.getFieldProps('headFamilyName')} isInvalid={formik.touched.headFamilyName && formik.errors.headFamilyName} />
							<Form.Control.Feedback type="invalid">
								{formik.touched.headFamilyName && formik.errors.headFamilyName}
							</Form.Control.Feedback>
						</InputGroup>
					</td>
				</tr>
				<tr>
					<th>Birthdate</th>
					<td>
						<InputGroup>
							<Form.Control type="date" {...formik.getFieldProps('birthDate')} isInvalid={formik.touched.birthDate && formik.errors.birthDate} />
							<Form.Control.Feedback type="invalid">
								{formik.touched.birthDate && formik.errors.birthDate}
							</Form.Control.Feedback>
						</InputGroup>
					</td>
				</tr>
				<tr>
					<th>Address</th>
					<td>
						<InputGroup>
							<Form.Control as="textarea" row={3} name="address" {...formik.getFieldProps('address')} isInvalid={formik.touched.address && formik.errors.address} />
							<Form.Control.Feedback type="invalid">
								{formik.touched.address && formik.errors.address}
							</Form.Control.Feedback>
						</InputGroup>
					</td>
				</tr>
				<tr>
					<th>Last Visit</th>
					<td>{lastVisit.format('LL')}</td>
				</tr>
				<tr>
					<td colSpan={2} style={{ textAlign: "right" }}>
						<Button variant="success" type="submit">Save</Button>
					</td>
				</tr>
			</tbody>
		</Table>
	</Form>
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
