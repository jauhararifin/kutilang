import { ContentState, convertToRaw, EditorState } from 'draft-js';
import draftToHtml from 'draftjs-to-html';
import { useFormik } from 'formik';
import htmlToDraft from 'html-to-draftjs';
import moment from 'moment';
import 'moment/locale/id';
import Noty from 'noty';
import { useEffect, useState } from 'react';
import Button from 'react-bootstrap/Button';
import Card from 'react-bootstrap/Card';
import Container from 'react-bootstrap/Container';
import Form from 'react-bootstrap/Form';
import FormControl from 'react-bootstrap/FormControl';
import InputGroup from 'react-bootstrap/InputGroup';
import Row from 'react-bootstrap/Row';
import Spinner from 'react-bootstrap/Spinner';
import Table from 'react-bootstrap/Table';
import { Editor } from "react-draft-wysiwyg";
import "react-draft-wysiwyg/dist/react-draft-wysiwyg.css";
import { BsPencil, BsTrash } from "react-icons/bs";
import { useHistory, useParams } from 'react-router-dom';
import * as Yup from 'yup';
import { deletePatient, deleteVisit, getPatient, getPatientVisits, insertVisit, postPatient, putPatient, putVisit } from '../service';

function showSuccessPopup(text) {
	new Noty({ text: text, layout: 'topCenter', type: 'success', theme: 'bootstrap-v4', timeout: 3000 }).show();
}

function showErrorPopup(text) {
	new Noty({ text: text, layout: 'topCenter', type: 'error', theme: 'bootstrap-v4', timeout: 3000 }).show();
}

function VisitPage() {
	const history = useHistory();
	const { patientId } = useParams()
	const isNewPatientPage = !patientId
	const [isLoading, setIsLoading] = useState(false)

	const [patient, setPatient] = useState(null)
	const [patientVisits, setPatientVisits] = useState([])
	const [nextCursor, setNextCursor] = useState('')

	useEffect(async () => {
		if (!patientId) return
		setIsLoading(true)

		const result = await getPatient(patientId)
		if (result['errorMessage'])
			showErrorPopup(result['errorMessage'])
		else
			setPatient(result['result'])

		const visitResp = await getPatientVisits(patientId, '')
		if (visitResp['errorMessage'])
			showErrorPopup(visitResp['errorMessage'])
		else {
			setPatientVisits(visitResp['result']['results'])
			setNextCursor(visitResp['result']['nextCursor'])
		}

		setIsLoading(false)
	}, [patientId])

	const nextPatientVisits = async () => {
		const visitResp = await getPatientVisits(patientId, nextCursor)
		if (visitResp['errorMessage'])
			showErrorPopup(visitResp['errorMessage'])
		else {
			setPatientVisits([...patientVisits, ...visitResp['result']['results']])
			setNextCursor(visitResp['result']['nextCursor'])
		}
	}

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

	const updateProfile = async (newPatient) => {
		setIsLoading(true)
		const resp = await putPatient({...newPatient, id: patient.id})
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Patient updated')
			setIsEditingProfile(false)
			setPatient(newPatient)
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

	const newVisit = async ({ diagnosis, date }) => {
		setIsLoading(true)
		const resp = await insertVisit({
			patientId: patient.id,
			date: date,
			diagnosis: diagnosis,
		})
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Visit inserted')
			setPatientVisits([resp['result'], ...patientVisits])
		}
		setIsLoading(false)
	}

	const updateVisit = async ({id, diagnosis}) => {
		setIsLoading(true)
		const resp = await putVisit({
			id: id,
			patientId: patient.id,
			diagnosis: diagnosis,
		})
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Visit updated')
			setPatientVisits(patientVisits.map(pv => pv.id == id ? resp['result'] : pv))
		}
		setIsLoading(false)
	}

	const removeVisit = async ({id, patientId}) => {
		setIsLoading(true)
		const resp = await deleteVisit({
			id: id,
			patientId: patientId,
		})
		if (resp['errorMessage'])
			showErrorPopup(resp['errorMessage'])
		else {
			showSuccessPopup('Visit deleted')
			setPatientVisits(patientVisits.filter(pv => pv.id != id ))
		}
		setIsLoading(false)
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
			<VisitEditor onSubmit={newVisit} />

			{patientVisits.length > 0 && <h5 style={{ textAlign: "center", marginBottom: 30, marginTop: 20 }}>History</h5>}
			{patientVisits.map(patientVisit => <Visit
				key={patientVisit.id}
				patientVisit={patientVisit}
				onSubmit={pv => updateVisit({...pv, id: patientVisit.id})}
				onDelete={_ => removeVisit({id: patientVisit.id, patientId: patientVisit.patientId})}
			/>)}

			{nextCursor && <Row style={{ marginBottom: 20 }}>
				<Button variant="link" onClick={nextPatientVisits}>Load more...</Button>
			</Row>}
		</>}
	</Container>
}

function Profile({ patient, onEdit, onDelete }) {
	return <Table striped bordered hover>
		<tbody>
			<tr>
				<th style={{ width: 200 }}>Nama</th>
				<td>{patient.name}</td>
			</tr>
			<tr>
				<th>Nama KK</th>
				<td>{patient.headFamilyName}</td>
			</tr>
			<tr>
				<th>Tanggal Lahir</th>
				<td>{moment(patient['birthDate']).format('LL')} ({moment().diff(patient['birthDate'], 'years')} years old)</td>
			</tr>
			<tr>
				<th>Alamat</th>
				<td>{patient['address']}</td>
			</tr>
			<tr>
				<th>Kunjungan Terakhir</th>
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

function Visit({onSubmit, onDelete, patientVisit}) {
	const [isEditing, setIsEditing] = useState(false)
	return isEditing ? 
		<VisitEditor onSubmit={onSubmit} onCancel={() => setIsEditing(false)} patientVisit={patientVisit} /> :
		<VisitView onDelete={onDelete} onEdit={() => setIsEditing(true)} patientVisit={patientVisit} />
}

function VisitView({ onDelete, onEdit, patientVisit }) {
	const date = moment(patientVisit['date'])
	const dateStr = date.format('LL') + ' (' + date.fromNow() + ')'

	return (
		<Card style={{ marginBottom: 20 }}>
			<Card.Header>{dateStr}</Card.Header>

			<Card.Body>
				<div dangerouslySetInnerHTML={{ __html: patientVisit.diagnosis }}></div>
			</Card.Body>

			<Card.Footer style={{ textAlign: "right" }}>
				<Button variant="link" onClick={onEdit}><BsPencil /></Button>
				<Button variant="link" onClick={onDelete}><BsTrash /></Button>
			</Card.Footer>
		</Card>
	)
}

function VisitEditor({ onSubmit, onCancel, patientVisit }) {
	const [editorState, setEditorState] = useState(
		!!patientVisit ? EditorState.createWithContent(
			ContentState.createFromBlockArray(
				htmlToDraft(patientVisit.diagnosis).contentBlocks
			)
		) : EditorState.createEmpty()
	);
	const [date, setDate] = useState(patientVisit ? patientVisit['date'] : new Date().toISOString().slice(0, 10))

	const submit = () => {
		onSubmit({
			'date': date,
			'diagnosis': draftToHtml(convertToRaw(editorState.getCurrentContent())),
		})
	}

	return (
		<Card style={{ marginBottom: 20 }}>
			<Card.Body style={{ padding: 10 }}>
				<FormControl type="date" value={date} onChange={ev => setDate(ev.target.value)} style={{marginBottom: 10}}/>
				<Editor
					editorState={editorState}
					onEditorStateChange={setEditorState}
					toolbar={{ options: ['inline', 'blockType', 'fontSize', 'list', 'textAlign', 'colorPicker', 'link', 'emoji', 'remove', 'history'], }}
					toolbarStyle={{ border: 0, padding: 0 }}
					editorStyle={{ padding: 10 }}
				/>
			</Card.Body>

			<Card.Footer style={{ textAlign: "right" }}>
				{onCancel && <Button variant="secondary" onClick={onCancel}>Cancel</Button>}
				{' '}<Button variant="primary" onClick={submit}>Submit</Button>
			</Card.Footer>
		</Card>
	)
}

export default VisitPage;
